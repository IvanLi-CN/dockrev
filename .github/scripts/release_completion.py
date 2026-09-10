#!/usr/bin/env python3
"""Validate the required Release completion check for one PR head."""

from __future__ import annotations

import argparse
import base64
import json
import os
import sys
import urllib.parse
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

import release_policy


class CompletionError(RuntimeError):
    pass


def api_json(api_root: str, token: str, path: str) -> Any:
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}{path}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "dockrev-release-completion",
        },
    )
    try:
        with urllib.request.urlopen(request) as response:
            return json.loads(response.read().decode(response.headers.get_content_charset() or "utf-8"))
    except urllib.error.HTTPError as error:
        raise CompletionError(f"GitHub API failed: {error.code}") from error


def workflow_runs_for_pr(
    api_root: str,
    token: str,
    repository: str,
    workflow_file: str,
    pr_number: int,
    source_sha: str | None = None,
) -> list[dict[str, Any]]:
    owner, name = repository.split("/", 1)
    result: list[dict[str, Any]] = []
    page = 1
    while True:
        runs = api_json(
            api_root,
            token,
            f"/repos/{owner}/{name}/actions/workflows/{workflow_file}/runs?per_page=100&page={page}",
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


def covered_product_boundary(
    api_root: str, token: str, repository: str, covered_merge_sha: str
) -> tuple[dict[str, Any], str]:
    owner, name = repository.split("/", 1)
    pulls = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{covered_merge_sha}/pulls")
    if not isinstance(pulls, list) or len(pulls) != 1:
        raise CompletionError("version-only release PR must cover exactly one merged product PR")
    pr = pulls[0]
    if pr.get("base", {}).get("ref") != "main" or pr.get("state") != "closed" or not pr.get("merged_at"):
        raise CompletionError("covered product boundary is not a merged main PR")
    if pr.get("merge_commit_sha") != covered_merge_sha:
        raise CompletionError("covered product boundary does not match the exact merge SHA")
    try:
        covered_intent = release_policy.parse_labels(
            [item.get("name") for item in pr.get("labels", []) if item.get("name")]
        )
    except release_policy.PolicyError as error:
        raise CompletionError(f"covered product labels are invalid: {error}") from error
    if not covered_intent["release_enabled"]:
        raise CompletionError("version-only release PR must cover a release-enabled product PR")
    head_sha = pr.get("head", {}).get("sha", "")
    release_policy.validate_sha(head_sha, "covered_product_head_sha")
    commit = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{head_sha}")
    trailers = release_policy.parse_trailers(commit.get("commit", {}).get("message", ""))
    if any(
        trailers.get(key)
        for key in ("Release-Mode", "Source-SHA", "Product-Version", "Release-Intent", "Covered-Product-Merge-SHA")
    ):
        raise CompletionError("covered product PR already has release identity")
    return pr, head_sha


def intent_from_trailer(value: str) -> dict[str, Any]:
    labels = value.split()
    if len(labels) != 2:
        raise CompletionError("release intent trailer must contain type and channel labels")
    try:
        return release_policy.parse_labels(labels)
    except release_policy.PolicyError as error:
        raise CompletionError(str(error)) from error


def version_at_commit(api_root: str, token: str, repository: str, commit_sha: str) -> str:
    owner, name = repository.split("/", 1)
    payload = api_json(
        api_root,
        token,
        f"/repos/{owner}/{name}/contents/VERSION?ref={urllib.parse.quote(commit_sha, safe='')}",
    )
    if payload.get("encoding") != "base64":
        raise CompletionError("VERSION content is not returned as base64")
    try:
        encoded_content = "".join(str(payload["content"]).split())
        version = base64.b64decode(encoded_content, validate=True).decode().strip()
    except (KeyError, ValueError, UnicodeDecodeError) as error:
        raise CompletionError("VERSION content is not valid UTF-8 base64") from error
    release_policy.parse_version(version)
    return version


def pull_request_changed_files(
    api_root: str, token: str, repository: str, pr_number: int
) -> list[str]:
    owner, name = repository.split("/", 1)
    files: list[str] = []
    page = 1
    while True:
        batch = api_json(
            api_root,
            token,
            f"/repos/{owner}/{name}/pulls/{pr_number}/files?per_page=100&page={page}",
        )
        if not isinstance(batch, list):
            raise CompletionError("GitHub PR file list is invalid")
        files.extend(str(item.get("filename")) for item in batch if item.get("filename"))
        if len(batch) < 100:
            return sorted(set(files))
        page += 1


def tag_is_reserved_by_other_pr(
    api_root: str, token: str, repository: str, version: str, pr_number: int
) -> bool:
    owner, name = repository.split("/", 1)
    for state in ("open", "closed"):
        page = 1
        while True:
            pulls = api_json(
                api_root,
                token,
                f"/repos/{owner}/{name}/pulls?state={state}&base=main&per_page=100&page={page}",
            )
            if not isinstance(pulls, list):
                raise CompletionError("GitHub PR list is invalid")
            for pull in pulls:
                if pull.get("number") == pr_number or (state == "closed" and not pull.get("merged_at")):
                    continue
                head_sha = pull.get("head", {}).get("sha", "")
                if not head_sha:
                    continue
                head = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{head_sha}")
                trailers = release_policy.parse_trailers(head.get("commit", {}).get("message", ""))
                if trailers.get("Release-Mode") in {"normal-preparation", "version-only-release-pr"} and trailers.get("Product-Version") == version:
                    return False
            if len(pulls) < 100:
                break
            page += 1
    return True


def version_exists_at_commit(
    api_root: str, token: str, repository: str, commit_sha: str
) -> bool:
    try:
        version_at_commit(api_root, token, repository, commit_sha)
    except CompletionError as error:
        if "GitHub API failed: 404" in str(error):
            return False
        raise
    return True


def validate_source_checks(payload: dict[str, Any]) -> None:
    required = {"ci_pr", "label_gate"}
    if required - set(payload):
        raise CompletionError("source check evidence is incomplete")
    for name in required:
        value = payload[name]
        if not isinstance(value, dict) or value.get("status") != "completed" or value.get("conclusion") != "success":
            raise CompletionError(f"source check did not pass: {name}")


def validate_completion(payload: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(payload, dict):
        raise CompletionError("completion input must be an object")
    intent = release_policy.parse_labels(payload.get("labels", []))
    if not intent["release_enabled"]:
        if "VERSION" in payload.get("changed_files", []):
            if payload.get("version_bootstrap") is not True:
                raise CompletionError("type:none PR cannot change VERSION after bootstrap")
            if not payload.get("version_file"):
                raise CompletionError("VERSION bootstrap must provide a non-empty version")
            if payload.get("base_version_exists") is not False:
                raise CompletionError("VERSION bootstrap requires VERSION to be absent on base")
            release_policy.parse_version(str(payload["version_file"]))
        if payload.get("release_mode") or payload.get("provenance") or payload.get("preparation"):
            raise CompletionError("type:none PR cannot carry release identity")
        return {"status": "pass", "release_enabled": False, "mode": "non-product"}
    source_sha = payload.get("source_sha")
    head_sha = payload.get("head_sha")
    release_policy.validate_sha(str(source_sha), "source_sha")
    release_policy.validate_sha(str(head_sha), "head_sha")
    validate_source_checks(payload.get("source_checks", {}))
    mode = payload.get("release_mode")
    preparation_payload = payload.get("preparation") or {}
    provenance_payload = payload.get("provenance") or {}
    expected_version = payload.get("version") or preparation_payload.get("version") or provenance_payload.get("product_version")
    if payload.get("version_file") != expected_version:
        raise CompletionError("PR head VERSION does not match release provenance")
    if mode == "normal-preparation":
        preparation = payload.get("preparation")
        if not isinstance(preparation, dict):
            raise CompletionError("normal preparation evidence is missing")
        release_policy.validate_preparation(preparation, source_sha=source_sha)
        preparation_intent = preparation.get("intent", {})
        if (
            preparation_intent.get("type_label") != intent.get("type_label")
            or preparation_intent.get("channel_label") != intent.get("channel_label")
        ):
            raise CompletionError("preparation release intent does not match current PR labels")
        if preparation.get("release_intent") != f"{intent['type_label']} {intent['channel_label']}":
            raise CompletionError("preparation release intent trailer is not frozen")
        if preparation.get("commit_sha") != head_sha:
            raise CompletionError("PR head is not the verified preparation commit")
        if payload.get("base_ref") != "main":
            raise CompletionError("release PR must target main")
        release_policy.validate_channel_version(str(preparation["version"]), intent["channel"])
        if preparation.get("source_version"):
            release_policy.validate_preparation_version(
                str(preparation["source_version"]), str(preparation["version"]), intent
            )
        if payload.get("tag_reserved") is not True:
            raise CompletionError("derived release tag is not reserved")
    elif mode == "version-only-release-pr":
        release_policy.validate_version_only(payload.get("changed_files", []), payload.get("provenance", {}), head_sha=head_sha)
        if source_sha != payload["provenance"].get("covered_product_head_sha"):
            raise CompletionError("version-only source SHA is not the covered product head")
        if payload["provenance"].get("release_intent") != f"{intent['type_label']} {intent['channel_label']}":
            raise CompletionError("version-only release intent does not match current PR labels")
        release_policy.validate_channel_version(str(payload["provenance"]["product_version"]), intent["channel"])
        if payload.get("tag_reserved") is not True:
            raise CompletionError("version-only release PR tag is not reserved")
    else:
        raise CompletionError("PR has no allowed release identity mode")
    return {
        "status": "pass",
        "release_enabled": True,
        "mode": mode,
        "source_sha": source_sha,
        "head_sha": head_sha,
        "version": payload.get("version") or preparation_payload.get("version") or provenance_payload.get("product_version"),
        "version_file": payload.get("version_file"),
        "intent": intent,
    }


def tag_is_available(api_root: str, token: str, repository: str, version: str) -> bool:
    owner, name = repository.split("/", 1)
    try:
        api_json(api_root, token, f"/repos/{owner}/{name}/git/ref/tags/v{version}")
    except CompletionError as error:
        if "GitHub API failed: 404" in str(error):
            return True
        raise
    return False


def version_reservation_is_owned(
    api_root: str, token: str, repository: str, version: str, pr_number: int, source_sha: str
) -> bool:
    owner, name = repository.split("/", 1)
    ref_name = f"release-reservation/v{version}"
    try:
        ref = api_json(api_root, token, f"/repos/{owner}/{name}/git/ref/heads/{urllib.parse.quote(ref_name, safe='')}")
    except CompletionError as error:
        if "GitHub API failed: 404" in str(error):
            return False
        raise
    reservation_sha = ref.get("object", {}).get("sha")
    if not reservation_sha:
        return False
    try:
        reservation = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{reservation_sha}")
        release_policy.validate_reservation(
            reservation, version=version, pr_number=pr_number, source_sha=source_sha
        )
    except (CompletionError, release_policy.PolicyError):
        return False
    return True


def load_github_completion(
    api_root: str, token: str, repository: str, pr_number: int, expected_head_sha: str | None = None
) -> dict[str, Any]:
    owner, name = repository.split("/", 1)
    pr = api_json(api_root, token, f"/repos/{owner}/{name}/pulls/{pr_number}")
    if pr.get("base", {}).get("ref") != "main" or pr.get("state") != "open":
        raise CompletionError("Release completion requires an open PR targeting main")
    labels = [item["name"] for item in pr.get("labels", []) if item.get("name")]
    intent = release_policy.parse_labels(labels)
    head_sha = pr.get("head", {}).get("sha", "")
    if expected_head_sha and head_sha != expected_head_sha:
        raise CompletionError("PR head changed during Release completion verification")
    source_sha = head_sha
    preparation = None
    provenance = None
    covered_pr = None
    commit = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{head_sha}")
    parents = [parent.get("sha") for parent in commit.get("parents", [])]
    files = sorted({entry.get("filename") for entry in commit.get("files", []) if entry.get("filename")})
    trailers = release_policy.parse_trailers(commit.get("commit", {}).get("message", ""))
    mode = trailers.get("Release-Mode")
    if not intent["release_enabled"]:
        if any(trailers.get(key) for key in ("Release-Mode", "Source-SHA", "Product-Version", "Release-Intent", "Covered-Product-Merge-SHA")):
            raise CompletionError("type:none PR cannot carry release identity")
        changed_files = pull_request_changed_files(api_root, token, repository, pr_number)
        if "VERSION" not in changed_files:
            return {"labels": labels}
        base_sha = pr.get("base", {}).get("sha")
        if not isinstance(base_sha, str) or not base_sha:
            raise CompletionError("VERSION bootstrap is missing the base SHA")
        base_version_exists = version_exists_at_commit(api_root, token, repository, base_sha)
        if base_version_exists:
            raise CompletionError("type:none PR cannot change an existing VERSION")
        return {
            "labels": labels,
            "changed_files": changed_files,
            "version_bootstrap": True,
            "base_version_exists": False,
            "version_file": version_at_commit(api_root, token, repository, head_sha),
        }
    if mode == "normal-preparation":
        if trailers.get("Covered-Product-Merge-SHA"):
            raise CompletionError("normal preparation identity cannot carry Covered-Product-Merge-SHA")
        trailer_intent = intent_from_trailer(trailers.get("Release-Intent", ""))
        source_sha = trailers.get("Source-SHA", "")
        source_version = version_at_commit(api_root, token, repository, source_sha)
        preparation = {
            "commit_sha": head_sha,
            "source_sha": source_sha,
            "version": trailers.get("Product-Version", ""),
            "intent": trailer_intent,
            "source_version": source_version,
            "release_intent": trailers.get("Release-Intent", ""),
            "release_mode": mode,
            "parents": parents,
            "changed_files": files,
            "verified": commit.get("commit", {}).get("verification", {}).get("verified") is True,
        }
    elif mode == "version-only-release-pr":
        if trailers.get("Source-SHA"):
            raise CompletionError("version-only release identity cannot carry Source-SHA")
        covered_merge_sha = trailers.get("Covered-Product-Merge-SHA", "")
        covered_pr, covered_head_sha = covered_product_boundary(api_root, token, repository, covered_merge_sha)
        source_sha = covered_head_sha
        provenance = {
            "covered_product_merge_sha": covered_merge_sha,
            "covered_product_pr_number": covered_pr.get("number", 0),
            "covered_product_head_sha": covered_head_sha,
            "covered_product_merged": True,
            "covered_product_has_identity": False,
            "product_version": trailers.get("Product-Version", ""),
            "release_intent": trailers.get("Release-Intent", ""),
            "release_mode": mode,
            "branch_head_sha": head_sha,
            "verified": commit.get("commit", {}).get("verification", {}).get("verified") is True,
        }
        files = pull_request_changed_files(api_root, token, repository, pr_number)
    else:
        provenance = None
    source_pr_number = covered_pr.get("number") if mode == "version-only-release-pr" and covered_pr else pr_number
    try:
        release_policy.validate_source_boundary(
            pull_request_changed_files(api_root, token, repository, int(source_pr_number))
        )
    except release_policy.PolicyError as error:
        raise CompletionError(str(error)) from error
    version_file = version_at_commit(api_root, token, repository, head_sha)
    check_sha = source_sha
    check_pr_number = covered_pr.get("number") if mode == "version-only-release-pr" else pr_number
    ci_runs = workflow_runs_for_pr(api_root, token, repository, "ci-pr.yml", check_pr_number)
    gate_sha = source_sha if mode == "normal-preparation" else head_sha
    label_runs = workflow_runs_for_pr(api_root, token, repository, "label-gate.yml", pr_number, gate_sha)
    ci_run = next((run for run in ci_runs if run.get("head_sha") == check_sha), None)
    label_gate = next((run for run in label_runs if run.get("conclusion") == "success"), None)
    if mode == "normal-preparation":
        version_for_tag = preparation["version"]
    elif mode == "version-only-release-pr":
        version_for_tag = provenance["product_version"]
    else:
        raise CompletionError("PR head has no accepted release provenance")
    tag_reserved = tag_is_available(api_root, token, repository, version_for_tag)
    if mode in {"normal-preparation", "version-only-release-pr"}:
        tag_reserved = tag_reserved and version_reservation_is_owned(
            api_root, token, repository, version_for_tag, pr_number, source_sha
        )
    tag_reserved = tag_reserved and tag_is_reserved_by_other_pr(
        api_root, token, repository, version_for_tag, pr_number
    )
    payload = {
        "labels": labels,
        "head_sha": head_sha,
        "source_sha": source_sha,
        "base_ref": pr.get("base", {}).get("ref"),
        "release_mode": mode,
        "preparation": preparation,
        "changed_files": files,
        "provenance": provenance,
        "version_file": version_file,
        "source_checks": {"ci_pr": ci_run or {}, "label_gate": label_gate or {}},
        "tag_reserved": tag_reserved,
    }
    return payload


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path)
    parser.add_argument("--repository")
    parser.add_argument("--pr-number", type=int)
    parser.add_argument("--head-sha")
    parser.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))
    parser.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    args = parser.parse_args()
    try:
        if args.input:
            payload = json.loads(args.input.read_text(encoding="utf-8"))
        else:
            if not args.repository or not args.pr_number or not args.token:
                raise CompletionError("repository, pr-number and token are required without --input")
            payload = load_github_completion(args.api_root, args.token, args.repository, args.pr_number, args.head_sha)
        print(json.dumps(validate_completion(payload), sort_keys=True))
        return 0
    except (CompletionError, release_policy.PolicyError, OSError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
