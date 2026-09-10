#!/usr/bin/env python3
"""Create and verify signed VERSION-only preparation commits on PR branches."""

from __future__ import annotations

import argparse
import base64
import json
import os
import re
import sys
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


def workflow_runs_for_sha(api_root: str, token: str, repository: str, workflow_file: str, source_sha: str) -> list[dict[str, Any]]:
    owner, name = repository_parts(repository)
    return api_request(
        api_root,
        token,
        "GET",
        f"/repos/{owner}/{name}/actions/workflows/{urllib.parse.quote(workflow_file, safe='')}/runs?head_sha={source_sha}&per_page=100",
    ).get("workflow_runs", [])


def source_ci_ready(api_root: str, token: str, repository: str, source_sha: str) -> None:
    ci_runs = [run for run in workflow_runs_for_sha(api_root, token, repository, "ci-pr.yml", source_sha) if run.get("head_sha") == source_sha]
    if not any(run.get("status") == "completed" and run.get("conclusion") == "success" for run in ci_runs):
        raise PreparationError("source SHA does not have a successful complete CI (PR) run")
    label_runs = [run for run in workflow_runs_for_sha(api_root, token, repository, "label-gate.yml", source_sha) if run.get("head_sha") == source_sha]
    if not any(run.get("status") == "completed" and run.get("conclusion") == "success" for run in label_runs):
        raise PreparationError("source SHA does not have a successful Label Gate check")


def current_version(api_root: str, token: str, repository: str, source_sha: str) -> str:
    owner, name = repository_parts(repository)
    payload = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/contents/VERSION?ref={urllib.parse.quote(source_sha)}")
    if payload.get("encoding") != "base64":
        raise PreparationError("VERSION content is not returned as base64")
    try:
        version = base64.b64decode(payload["content"], validate=True).decode().strip()
    except (KeyError, ValueError, UnicodeDecodeError) as error:
        raise PreparationError("VERSION content is not valid UTF-8 base64") from error
    release_policy.parse_version(version)
    return version


def reserve_tag(api_root: str, token: str, repository: str, version: str, pr_number: int) -> None:
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

    open_pulls = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/pulls?state=open&base=main&per_page=100")
    for pull in open_pulls if isinstance(open_pulls, list) else []:
        if pull.get("number") == pr_number:
            continue
        head_sha = pull.get("head", {}).get("sha", "")
        if not re.fullmatch(r"[0-9a-f]{40}", head_sha):
            continue
        head_commit = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/commits/{head_sha}")
        trailers = release_policy.parse_trailers(head_commit.get("commit", {}).get("message", ""))
        if trailers.get("Release-Mode") == "normal-preparation" and trailers.get("Product-Version") == version:
            raise PreparationError(
                f"release version {version} is already reserved by open PR #{pull.get('number')}"
            )


def expected_version(intent: dict[str, Any], base_version: str, exact_version: str | None) -> str:
    if intent["type"] == "patch":
        if exact_version:
            raise PreparationError("type:patch preparation does not accept an exact version")
        version = release_policy.next_patch(base_version)
    elif intent["type"] in {"major", "minor"}:
        if not exact_version:
            raise PreparationError("major/minor preparation requires an explicit exact version")
        version = exact_version
    else:
        raise PreparationError("type:none does not create a release preparation commit")
    release_policy.validate_channel_version(version, intent["channel"])
    return version


def create_commit(
    api_root: str,
    token: str,
    repository: str,
    branch: str,
    source_sha: str,
    version: str,
    intent: dict[str, Any],
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
    commit = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/commits/{commit_sha}")
    ref = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/git/ref/heads/{urllib.parse.quote(branch, safe='')}")
    head_sha = ref.get("object", {}).get("sha")
    files = sorted({item.get("filename") for item in commit.get("files", []) if item.get("filename")})
    verification = commit.get("commit", {}).get("verification", {})
    trailers = release_policy.parse_trailers(commit.get("commit", {}).get("message", ""))
    payload = {
        "commit_sha": commit_sha,
        "source_sha": source_sha,
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
    intent = release_policy.parse_labels([str(item.get("name")) for item in pr.get("labels", []) if item.get("name")])
    if not intent["release_enabled"]:
        write_json(args.output, {"release_enabled": False, "pr_number": args.pr_number, "source_sha": source_sha, "reason": "type:none"})
        return 0
    source_ci_ready(args.api_root, args.token, args.repository, source_sha)
    base_version = current_version(args.api_root, args.token, args.repository, source_sha)
    version = expected_version(intent, base_version, args.exact_version)
    reserve_tag(args.api_root, args.token, args.repository, version, args.pr_number)
    commit_sha = create_commit(args.api_root, args.token, args.repository, pr["head"]["ref"], source_sha, version, intent)
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
