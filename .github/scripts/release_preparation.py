#!/usr/bin/env python3
"""Create and verify signed VERSION-only preparation commits on PR branches."""

from __future__ import annotations

import argparse
import base64
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

import release_policy


class PreparationError(RuntimeError):
    pass


def api_request(api_root: str, token: str, method: str, path: str, payload: dict[str, Any] | None = None) -> Any:
    url = path if path.startswith("http") else f"{api_root.rstrip('/')}{path}"
    data = None if payload is None else json.dumps(payload).encode()
    request = urllib.request.Request(
        url,
        data=data,
        method=method,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "dockrev-release-preparation",
            **({"Content-Type": "application/json"} if data else {}),
        },
    )
    try:
        with urllib.request.urlopen(request) as response:
            body = response.read().decode(response.headers.get_content_charset() or "utf-8")
            return json.loads(body) if body else {}
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        raise PreparationError(f"GitHub API {method} {path} failed: {error.code}: {detail[:500]}") from error


def graphql(api_root: str, token: str, query: str, variables: dict[str, Any]) -> dict[str, Any]:
    payload = api_request(api_root, token, "POST", f"{api_root.rstrip('/')}/graphql", {"query": query, "variables": variables})
    if payload.get("errors"):
        raise PreparationError(f"GraphQL mutation failed: {payload['errors']}")
    return payload.get("data", {})


def repository_parts(repository: str) -> tuple[str, str]:
    parts = repository.split("/", 1)
    if len(parts) != 2 or not all(parts):
        raise PreparationError("repository must be owner/name")
    return parts[0], parts[1]


def labels_for_pr(api_root: str, token: str, repository: str, number: int) -> list[str]:
    owner, name = repository_parts(repository)
    payload = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/pulls/{number}")
    return [str(item.get("name")) for item in payload.get("labels", []) if item.get("name")]


def pull_request(api_root: str, token: str, repository: str, number: int) -> dict[str, Any]:
    owner, name = repository_parts(repository)
    return api_request(api_root, token, "GET", f"/repos/{owner}/{name}/pulls/{number}")


def workflow_runs_for_pr(
    api_root: str,
    token: str,
    repository: str,
    workflow_file: str,
    pr_number: int,
    source_sha: str | None = None,
) -> list[dict[str, Any]]:
    owner, name = repository_parts(repository)
    result: list[dict[str, Any]] = []
    page = 1
    while True:
        runs = api_request(
            api_root,
            token,
            "GET",
            f"/repos/{owner}/{name}/actions/workflows/{urllib.parse.quote(workflow_file, safe='')}/runs?per_page=100&page={page}",
        ).get("workflow_runs", [])
        result.extend(
            run for run in runs
            if any(
                item.get("number") == pr_number
                and (
                    source_sha is None
                    or run.get("head_sha") == source_sha
                    or item.get("head", {}).get("sha") == source_sha
                )
                for item in run.get("pull_requests", [])
            )
        )
        if len(runs) < 100:
            return result
        page += 1


def pull_request_changed_files(api_root: str, token: str, repository: str, number: int) -> list[str]:
    owner, name = repository_parts(repository)
    files: list[str] = []
    page = 1
    while True:
        batch = api_request(
            api_root,
            token,
            "GET",
            f"/repos/{owner}/{name}/pulls/{number}/files?per_page=100&page={page}",
        )
        files.extend(str(item.get("filename")) for item in batch if item.get("filename"))
        if len(batch) < 100:
            return sorted(set(files))
        page += 1


def source_ci_ready(
    api_root: str,
    token: str,
    repository: str,
    pr_number: int,
    source_sha: str,
    *,
    expected_pr_updated_at: str | None = None,
    expected_intent: dict[str, Any] | None = None,
    require_current_head: bool = True,
    require_unchanged_pr: bool = False,
) -> str:
    pr = pull_request(api_root, token, repository, pr_number)
    current_head = str(pr.get("head", {}).get("sha", ""))
    if require_current_head and current_head != source_sha:
        raise PreparationError("PR head changed while source checks were being verified")
    current_updated_at = str(pr.get("updated_at", ""))
    if require_unchanged_pr and expected_pr_updated_at and current_updated_at != expected_pr_updated_at:
        raise PreparationError("PR labels or metadata changed while source checks were being verified")
    if expected_intent is not None:
        try:
            current_intent = release_policy.parse_labels(
                [str(item.get("name")) for item in pr.get("labels", []) if item.get("name")]
            )
        except release_policy.PolicyError as error:
            raise PreparationError(str(error)) from error
        if any(
            current_intent[field] != expected_intent[field]
            for field in ("type_label", "channel_label", "components")
        ):
            raise PreparationError("PR release intent labels changed while source checks were being verified")
    label_updated_at = expected_pr_updated_at or current_updated_at
    if not label_updated_at:
        raise PreparationError("PR metadata is missing updated_at for Label Gate binding")
    try:
        release_policy.validate_source_boundary(pull_request_changed_files(api_root, token, repository, pr_number))
    except release_policy.PolicyError as error:
        raise PreparationError(str(error)) from error
    ci_runs = [run for run in workflow_runs_for_pr(api_root, token, repository, "ci-pr.yml", pr_number) if run.get("head_sha") == source_sha]
    if not any(run.get("status") == "completed" and run.get("conclusion") == "success" for run in ci_runs):
        raise PreparationError("source SHA does not have a successful complete CI (PR) run")
    label_runs = workflow_runs_for_pr(api_root, token, repository, "label-gate.yml", pr_number, source_sha)
    if not any(
        run.get("status") == "completed"
        and run.get("conclusion") == "success"
        and str(run.get("created_at", "")) >= label_updated_at
        for run in label_runs
    ):
        raise PreparationError("PR does not have a successful Label Gate check")
    return label_updated_at


def current_version(api_root: str, token: str, repository: str, source_sha: str) -> str:
    owner, name = repository_parts(repository)
    payload = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/contents/VERSION?ref={urllib.parse.quote(source_sha)}")
    if payload.get("encoding") != "base64":
        raise PreparationError("VERSION content is not returned as base64")
    try:
        encoded_content = "".join(str(payload["content"]).split())
        version = base64.b64decode(encoded_content, validate=True).decode().strip()
    except (KeyError, ValueError, UnicodeDecodeError) as error:
        raise PreparationError("VERSION content is not valid UTF-8 base64") from error
    release_policy.parse_version(version)
    return version


def pull_requests(api_root: str, token: str, repository: str, state: str) -> list[dict[str, Any]]:
    owner, name = repository_parts(repository)
    result: list[dict[str, Any]] = []
    page = 1
    while True:
        batch = api_request(
            api_root,
            token,
            "GET",
            f"/repos/{owner}/{name}/pulls?state={state}&base=main&per_page=100&page={page}",
        )
        if not isinstance(batch, list):
            raise PreparationError("GitHub PR list is invalid")
        result.extend(batch)
        if len(batch) < 100:
            return result
        page += 1


def covered_product_head_sha(api_root: str, token: str, repository: str, merge_sha: str) -> str:
    owner, name = repository_parts(repository)
    pulls = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/commits/{merge_sha}/pulls")
    if not isinstance(pulls, list) or len(pulls) != 1:
        raise PreparationError("version-only release PR must cover exactly one merged product PR")
    product = pulls[0]
    if product.get("base", {}).get("ref") != "main" or product.get("state") != "closed" or not product.get("merged_at"):
        raise PreparationError("covered product boundary is not a merged main PR")
    if product.get("merge_commit_sha") != merge_sha:
        raise PreparationError("covered product boundary does not match the exact merge SHA")
    try:
        product_intent = release_policy.parse_labels(
            [item.get("name") for item in product.get("labels", []) if item.get("name")]
        )
    except release_policy.PolicyError as error:
        raise PreparationError(f"covered product labels are invalid: {error}") from error
    if not product_intent["release_enabled"]:
        raise PreparationError("version-only release PR must cover a release-enabled product PR")
    head_sha = product.get("head", {}).get("sha", "")
    release_policy.validate_sha(head_sha, "covered_product_head_sha")
    commit = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/commits/{head_sha}")
    trailers = release_policy.parse_trailers(commit.get("commit", {}).get("message", ""))
    if any(
        trailers.get(key)
        for key in ("Release-Mode", "Source-SHA", "Source-PR-Updated-At", "Product-Version", "Release-Intent", "Covered-Product-Merge-SHA")
    ):
        raise PreparationError("covered product PR already has release identity")
    return head_sha


def reserve_tag(api_root: str, token: str, repository: str, version: str, pr_number: int, source_sha: str) -> bool:
    owner, name = repository_parts(repository)
    try:
        existing = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/git/ref/tags/v{version}")
    except PreparationError as error:
        if " 404:" in str(error):
            pass
        else:
            raise
    else:
        raise PreparationError(f"release tag v{version} already exists and cannot be reserved: {existing}")

    for state in ("open", "closed"):
        for pull in pull_requests(api_root, token, repository, state):
            if pull.get("number") == pr_number or (state == "closed" and not pull.get("merged_at")):
                continue
            head_sha = pull.get("head", {}).get("sha", "")
            if not re.fullmatch(r"[0-9a-f]{40}", head_sha):
                continue
            head_commit = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/commits/{head_sha}")
            trailers = release_policy.parse_trailers(head_commit.get("commit", {}).get("message", ""))
            if trailers.get("Release-Mode") in {"normal-preparation", "version-only-release-pr"} and trailers.get("Product-Version") == version:
                raise PreparationError(
                    f"release version {version} is already reserved by PR #{pull.get('number')}"
                )
    return reserve_version_ref(api_root, token, repository, version, pr_number, source_sha)


def reserve_version_ref(
    api_root: str, token: str, repository: str, version: str, pr_number: int, source_sha: str
) -> bool:
    """CAS one version ref to an owner-stamped reservation commit."""
    owner, name = repository_parts(repository)
    ref_name = f"release-reservation/v{version}"
    path = f"/repos/{owner}/{name}/git/ref/heads/{urllib.parse.quote(ref_name, safe='')}"
    try:
        existing = api_request(api_root, token, "GET", path)
    except PreparationError as error:
        if " 404:" not in str(error):
            raise
        source = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/commits/{source_sha}")
        tree_sha = source.get("commit", {}).get("tree", {}).get("sha")
        if not isinstance(tree_sha, str) or not re.fullmatch(r"[0-9a-f]{40}", tree_sha):
            raise PreparationError("source commit tree SHA is invalid")
        reservation = api_request(
            api_root,
            token,
            "POST",
            f"/repos/{owner}/{name}/git/commits",
            {
                "message": (
                    f"Reserve release version v{version}\n\n"
                    f"Release-Reservation-Version: {version}\n"
                    f"Release-Reservation-PR: {pr_number}\n"
                    f"Release-Reservation-Source-SHA: {source_sha}"
                ),
                "tree": tree_sha,
                "parents": [source_sha],
            },
        )
        reservation_sha = reservation.get("sha")
        if not isinstance(reservation_sha, str) or not re.fullmatch(r"[0-9a-f]{40}", reservation_sha):
            raise PreparationError("release reservation commit SHA is invalid")
        try:
            api_request(
                api_root,
                token,
                "POST",
                f"/repos/{owner}/{name}/git/refs",
                {"ref": f"refs/heads/{ref_name}", "sha": reservation_sha},
            )
            return True
        except PreparationError as create_error:
            if " 422:" not in str(create_error):
                raise
            existing = api_request(api_root, token, "GET", path)
    reservation_sha = existing.get("object", {}).get("sha")
    if not isinstance(reservation_sha, str):
        raise PreparationError("release reservation ref has no commit SHA")
    reservation_commit = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/commits/{reservation_sha}")
    try:
        release_policy.validate_reservation(reservation_commit, version=version, pr_number=pr_number, source_sha=source_sha)
    except release_policy.PolicyError as error:
        raise PreparationError(str(error)) from error
    return False


def delete_reservation_ref(api_root: str, token: str, repository: str, version: str) -> None:
    owner, name = repository_parts(repository)
    ref_name = f"release-reservation/v{version}"
    path = f"/repos/{owner}/{name}/git/refs/heads/{urllib.parse.quote(ref_name, safe='')}"
    try:
        api_request(api_root, token, "DELETE", path)
    except PreparationError as error:
        if " 404:" not in str(error):
            raise


def expected_version(intent: dict[str, Any], base_version: str, exact_version: str | None) -> str:
    if intent["type"] == "patch":
        if intent["channel"] == "stable":
            if exact_version:
                raise PreparationError("stable type:patch preparation does not accept an exact version")
            version = release_policy.next_patch(base_version)
        else:
            if not exact_version:
                raise PreparationError("beta/dev type:patch preparation requires an explicit exact version")
            version = exact_version
    elif intent["type"] in {"major", "minor"}:
        if not exact_version:
            raise PreparationError("major/minor preparation requires an explicit exact version")
        version = exact_version
    else:
        raise PreparationError("type:none does not create a release preparation commit")
    try:
        release_policy.validate_preparation_version(base_version, version, intent)
    except release_policy.PolicyError as error:
        raise PreparationError(str(error)) from error
    return version


def is_existing_preparation(trailers: dict[str, str]) -> bool:
    return trailers.get("Release-Mode") == "normal-preparation" and bool(
        trailers.get("Source-SHA") and trailers.get("Product-Version")
    )


def is_version_only_release(trailers: dict[str, str]) -> bool:
    return trailers.get("Release-Mode") == "version-only-release-pr" and bool(
        trailers.get("Covered-Product-Merge-SHA")
        and trailers.get("Product-Version")
        and trailers.get("Release-Intent")
    )


def create_commit(
    api_root: str,
    token: str,
    repository: str,
    branch: str,
    source_sha: str,
    version: str,
    intent: dict[str, Any],
    source_pr_updated_at: str,
) -> str:
    encoded = base64.b64encode((version + "\n").encode()).decode()
    query = """
    mutation($input: CreateCommitOnBranchInput!) {
      createCommitOnBranch(input: $input) {
        commit { oid }
      }
    }
    """
    body = "\n".join(
        [
            "Prepare release identity",
            "",
            f"Source-SHA: {source_sha}",
            f"Source-PR-Updated-At: {source_pr_updated_at}",
            f"Product-Version: {version}",
            f"Release-Intent: {intent['type_label']} {intent['channel_label']}",
            "Release-Mode: normal-preparation",
        ]
    )
    variables = {
        "input": {
            "branch": {"repositoryNameWithOwner": repository, "branchName": branch},
            "expectedHeadOid": source_sha,
            "message": {"headline": "chore(release): prepare VERSION", "body": body},
            "fileChanges": {"additions": [{"path": "VERSION", "contents": encoded}]},
        }
    }
    data = graphql(f"{api_root.rstrip('/')}", token, query, variables)
    oid = (((data.get("createCommitOnBranch") or {}).get("commit") or {}).get("oid"))
    if not isinstance(oid, str) or not re.fullmatch(r"[0-9a-f]{40}", oid):
        raise PreparationError("createCommitOnBranch did not return a commit OID")
    return oid


def inspect_commit(api_root: str, token: str, repository: str, branch: str, commit_sha: str, source_sha: str, version: str, intent: dict[str, Any]) -> dict[str, Any]:
    owner, name = repository_parts(repository)
    commit = None
    for attempt in range(5):
        candidate = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/commits/{commit_sha}")
        commit = candidate
        verification = candidate.get("commit", {}).get("verification", {})
        if verification.get("verified") is True or attempt == 4:
            break
        time.sleep(2)
    assert commit is not None
    ref = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/git/ref/heads/{urllib.parse.quote(branch, safe='')}")
    head_sha = ref.get("object", {}).get("sha")
    files = sorted({item.get("filename") for item in commit.get("files", []) if item.get("filename")})
    verification = commit.get("commit", {}).get("verification", {})
    trailers = release_policy.parse_trailers(commit.get("commit", {}).get("message", ""))
    payload = {
        "commit_sha": commit_sha,
        "source_sha": source_sha,
        "source_pr_updated_at": trailers.get("Source-PR-Updated-At", ""),
        "version": version,
        "intent": intent,
        "release_mode": trailers.get("Release-Mode", ""),
        "parents": [parent.get("sha") for parent in commit.get("parents", [])],
        "changed_files": files,
        "verified": verification.get("verified") is True,
        "branch_head_sha": head_sha,
        "trailers": trailers,
    }
    release_policy.validate_preparation(payload, source_sha=source_sha)
    if head_sha != commit_sha:
        raise PreparationError("PR branch head drifted after preparation commit")
    if trailers.get("Source-SHA") != source_sha or trailers.get("Product-Version") != version:
        raise PreparationError("preparation provenance trailers do not match source/version")
    if not trailers.get("Source-PR-Updated-At"):
        raise PreparationError("preparation source PR timestamp trailer is missing")
    if trailers.get("Release-Intent") != f"{intent['type_label']} {intent['channel_label']}":
        raise PreparationError("preparation release intent trailer does not match labels")
    return payload


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def create(args: argparse.Namespace) -> int:
    pr = pull_request(args.api_root, args.token, args.repository, args.pr_number)
    if pr.get("state") != "open" or pr.get("base", {}).get("ref") != "main":
        raise PreparationError("preparation requires an open PR targeting main")
    head_repo = (pr.get("head", {}).get("repo") or {}).get("full_name")
    if head_repo != args.repository:
        raise PreparationError("fork PR branches are not eligible for GitHub-native preparation")
    source_sha = pr.get("head", {}).get("sha", "")
    release_policy.validate_sha(source_sha, "source_sha")
    owner, name = repository_parts(args.repository)
    head_commit = api_request(args.api_root, args.token, "GET", f"/repos/{owner}/{name}/commits/{source_sha}")
    head_trailers = release_policy.parse_trailers(head_commit.get("commit", {}).get("message", ""))
    intent = release_policy.parse_labels([str(item.get("name")) for item in pr.get("labels", []) if item.get("name")])
    if any(
        head_trailers.get(key)
        for key in ("Release-Mode", "Source-SHA", "Source-PR-Updated-At", "Product-Version", "Release-Intent", "Covered-Product-Merge-SHA")
    ) and head_trailers.get("Release-Mode") not in {"normal-preparation", "version-only-release-pr"}:
        raise PreparationError("PR head contains malformed release identity trailers")
    if head_trailers.get("Release-Mode") == "normal-preparation" and not is_existing_preparation(head_trailers):
        raise PreparationError("normal preparation identity is incomplete")
    if head_trailers.get("Release-Mode") == "normal-preparation" and head_trailers.get("Covered-Product-Merge-SHA"):
        raise PreparationError("normal preparation identity cannot carry Covered-Product-Merge-SHA")
    if head_trailers.get("Release-Mode") == "version-only-release-pr" and not is_version_only_release(head_trailers):
        raise PreparationError("version-only release identity is incomplete")
    if is_version_only_release(head_trailers):
        if not intent["release_enabled"]:
            raise PreparationError("type:none PR cannot retain release-only identity")
        if head_trailers.get("Source-SHA"):
            raise PreparationError("version-only release identity cannot carry Source-SHA")
        if head_trailers.get("Release-Intent") != f"{intent['type_label']} {intent['channel_label']}":
            raise PreparationError("existing release-only intent does not match current PR labels")
        version = head_trailers["Product-Version"]
        try:
            release_policy.parse_version(version)
            release_policy.validate_channel_version(version, intent["channel"])
        except release_policy.PolicyError as error:
            raise PreparationError(str(error)) from error
        covered_head_sha = covered_product_head_sha(
            args.api_root, args.token, args.repository, head_trailers["Covered-Product-Merge-SHA"]
        )
        reserve_tag(args.api_root, args.token, args.repository, version, args.pr_number, covered_head_sha)
        write_json(
            args.output,
            {
                "schema_version": 1,
                "release_enabled": True,
                "pr_number": args.pr_number,
                "source_sha": covered_head_sha,
                "preparation_commit_sha": source_sha,
                "version": version,
                "release_tag": f"v{version}",
                "intent": intent,
                "release_mode": "version-only-release-pr",
                "skipped": "already-version-only",
            },
        )
        return 0
    if is_existing_preparation(head_trailers):
        if not intent["release_enabled"]:
            raise PreparationError("type:none PR cannot retain release preparation identity")
        existing_source_sha = head_trailers["Source-SHA"]
        existing_version = head_trailers["Product-Version"]
        release_policy.validate_sha(existing_source_sha, "preparation source_sha")
        release_policy.parse_version(existing_version)
        release_intent = release_policy.parse_labels(head_trailers.get("Release-Intent", "").split())
        if release_intent["type_label"] != intent["type_label"] or release_intent["channel_label"] != intent["channel_label"]:
            raise PreparationError("existing preparation release intent does not match current PR labels")
        source_version = current_version(args.api_root, args.token, args.repository, existing_source_sha)
        source_ci_ready(
            args.api_root, args.token, args.repository, args.pr_number, existing_source_sha,
            expected_pr_updated_at=head_trailers.get("Source-PR-Updated-At"),
            expected_intent=intent,
            require_current_head=False,
        )
        existing = inspect_commit(
            args.api_root,
            args.token,
            args.repository,
            pr["head"]["ref"],
            source_sha,
            existing_source_sha,
            existing_version,
            intent,
        )
        release_policy.validate_preparation_version(source_version, existing_version, intent)
        reserve_tag(args.api_root, args.token, args.repository, existing_version, args.pr_number, existing_source_sha)
        write_json(
            args.output,
            {
                "schema_version": 1,
                "release_enabled": True,
                "pr_number": args.pr_number,
                "source_sha": existing_source_sha,
                "preparation_commit_sha": source_sha,
                "version": existing_version,
                "release_tag": f"v{existing_version}",
                "intent": intent,
                "release_mode": "normal-preparation",
                "preparation": existing,
                "skipped": "already-prepared",
            },
        )
        return 0
    if not intent["release_enabled"]:
        write_json(args.output, {"release_enabled": False, "pr_number": args.pr_number, "source_sha": source_sha, "reason": "type:none"})
        return 0
    source_pr_updated_at = source_ci_ready(
        args.api_root,
        args.token,
        args.repository,
        args.pr_number,
        source_sha,
        expected_pr_updated_at=str(pr.get("updated_at", "")),
        expected_intent=intent,
        require_unchanged_pr=True,
    )
    base_version = current_version(args.api_root, args.token, args.repository, source_sha)
    version = expected_version(intent, base_version, args.exact_version)
    reservation_created = reserve_tag(args.api_root, args.token, args.repository, version, args.pr_number, source_sha)
    commit_sha = None
    try:
        source_ci_ready(
            args.api_root,
            args.token,
            args.repository,
            args.pr_number,
            source_sha,
            expected_pr_updated_at=source_pr_updated_at,
            expected_intent=intent,
            require_unchanged_pr=True,
        )
        commit_sha = create_commit(
            args.api_root, args.token, args.repository, pr["head"]["ref"], source_sha, version, intent,
            source_pr_updated_at,
        )
    except PreparationError:
        if reservation_created and commit_sha is None:
            delete_reservation_ref(args.api_root, args.token, args.repository, version)
        raise
    preparation = inspect_commit(args.api_root, args.token, args.repository, pr["head"]["ref"], commit_sha, source_sha, version, intent)
    payload = {
        "schema_version": 1,
        "release_enabled": True,
        "pr_number": args.pr_number,
        "source_sha": source_sha,
        "preparation_commit_sha": commit_sha,
        "version": version,
        "release_tag": f"v{version}",
        "intent": intent,
        "release_mode": "normal-preparation",
        "preparation": preparation,
    }
    write_json(args.output, payload)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    create_parser = sub.add_parser("create")
    create_parser.add_argument("--pr-number", type=int, required=True)
    create_parser.add_argument("--repository", required=True)
    create_parser.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))
    create_parser.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    create_parser.add_argument("--exact-version")
    create_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not args.token:
        print("GITHUB_TOKEN is required", file=sys.stderr)
        return 2
    try:
        if args.command == "create":
            return create(args)
        raise PreparationError("unsupported preparation command")
    except (PreparationError, release_policy.PolicyError, OSError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
