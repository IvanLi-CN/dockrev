#!/usr/bin/env python3
"""Allocate a version identity from main:VERSION and open its protected PR."""

from __future__ import annotations

import argparse
import base64
import json
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


def api_request(
    api_root: str, token: str, method: str, path: str,
    payload: dict[str, Any] | None = None,
) -> Any:
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
            "User-Agent": "dockrev-manual-version-release",
            **({"Content-Type": "application/json"} if data else {}),
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            body = response.read().decode(response.headers.get_content_charset() or "utf-8")
            return json.loads(body) if body else {}
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        if error.code == 404:
            return None
        raise PreparationError(f"GitHub API {method} {path} failed: {error.code}: {detail[:500]}") from error
    except urllib.error.URLError as error:
        raise PreparationError(f"GitHub API request failed: {error.reason}") from error


def graphql(api_root: str, token: str, query: str, variables: dict[str, Any]) -> dict[str, Any]:
    response = api_request(api_root, token, "POST", f"{api_root.rstrip('/')}/graphql", {"query": query, "variables": variables})
    if response.get("errors"):
        raise PreparationError(f"GitHub commit creation failed: {response['errors']}")
    return response.get("data", {})


def repository_parts(repository: str) -> tuple[str, str]:
    owner, separator, name = repository.partition("/")
    if not separator or not owner or not name:
        raise PreparationError("repository must be owner/name")
    return owner, name


def branch_ref(api_root: str, token: str, repository: str, branch: str) -> str | None:
    owner, name = repository_parts(repository)
    quoted = urllib.parse.quote(branch, safe="/")
    response = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/git/ref/heads/{quoted}")
    if response is None:
        return None
    sha = response.get("object", {}).get("sha")
    release_policy.validate_sha(str(sha), "branch tip SHA")
    return str(sha)


def create_branch(api_root: str, token: str, repository: str, branch: str, sha: str) -> str:
    owner, name = repository_parts(repository)
    response = api_request(
        api_root, token, "POST", f"/repos/{owner}/{name}/git/refs",
        {"ref": f"refs/heads/{branch}", "sha": sha},
    )
    if response is None:
        live = branch_ref(api_root, token, repository, branch)
        if live is None:
            raise PreparationError("release preparation branch creation could not be confirmed")
        return live
    created_sha = response.get("object", {}).get("sha")
    release_policy.validate_sha(str(created_sha), "created branch SHA")
    return str(created_sha)


def version_at_ref(api_root: str, token: str, repository: str, ref: str) -> str:
    owner, name = repository_parts(repository)
    quoted = urllib.parse.quote(ref, safe="")
    response = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/contents/VERSION?ref={quoted}")
    if not isinstance(response, dict) or response.get("encoding") != "base64":
        raise PreparationError(f"{ref}:VERSION is missing or is not base64 content")
    try:
        encoded = "".join(str(response["content"]).split())
        version = release_policy.parse_version_file(
            base64.b64decode(encoded, validate=True).decode("utf-8")
        )
    except (KeyError, ValueError, UnicodeDecodeError, release_policy.PolicyError) as error:
        raise PreparationError(f"{ref}:VERSION is invalid") from error
    return version


def main_sha(api_root: str, token: str, repository: str) -> str:
    owner, name = repository_parts(repository)
    response = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/git/ref/heads/main")
    if not isinstance(response, dict):
        raise PreparationError("main branch could not be read")
    sha = str((response.get("object") or {}).get("sha", ""))
    release_policy.validate_sha(sha, "main SHA")
    return sha


def open_release_preparation_pull_requests(
    api_root: str, token: str, repository: str,
) -> list[dict[str, Any]]:
    owner, name = repository_parts(repository)
    prefix = "release-preparation/v"
    matches: list[dict[str, Any]] = []
    page = 1
    while True:
        query = urllib.parse.urlencode({"state": "open", "base": "main", "per_page": 100, "page": page})
        pulls = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/pulls?{query}")
        if not isinstance(pulls, list):
            raise PreparationError("GitHub did not return the open pull request list")
        for pull in pulls:
            if not isinstance(pull, dict):
                raise PreparationError("GitHub returned an invalid pull request entry")
            head = pull.get("head") or {}
            if pull.get("base", {}).get("ref") == "main" and str(head.get("ref", "")).startswith(prefix):
                matches.append(pull)
        if len(pulls) < 100:
            return matches
        page += 1


def assert_no_competing_release_preparation(
    api_root: str, token: str, repository: str, expected_branch: str,
) -> None:
    pulls = open_release_preparation_pull_requests(api_root, token, repository)
    branches = [str((pull.get("head") or {}).get("ref", "")) for pull in pulls]
    expected_count = branches.count(expected_branch)
    if expected_count > 1:
        raise PreparationError("multiple open PRs exist for one release identity branch")
    competing = sorted(set(branch for branch in branches if branch != expected_branch))
    if competing:
        raise PreparationError(
            "another release identity PR is open; finish it before preparing a new version: "
            + ", ".join(competing)
        )


def assert_baseline_current(
    api_root: str, token: str, repository: str, baseline: str,
) -> None:
    latest_sha = main_sha(api_root, token, repository)
    latest_baseline = version_at_ref(api_root, token, repository, latest_sha)
    if latest_baseline != baseline:
        raise PreparationError(
            f"main:VERSION changed during release preparation ({baseline} -> {latest_baseline}); retry from the new baseline"
        )


def inspect_identity(
    api_root: str, token: str, repository: str, identity_sha: str,
    main_sha: str, baseline: str, decision: dict[str, str],
) -> dict[str, Any]:
    owner, name = repository_parts(repository)
    commit = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/commits/{identity_sha}")
    if not isinstance(commit, dict):
        raise PreparationError("reserved identity commit is unavailable")
    metadata = commit.get("commit") or {}
    if (metadata.get("verification") or {}).get("verified") is not True:
        raise PreparationError("release identity commit is not signed")
    parents = [str(item.get("sha", "")) for item in commit.get("parents", [])]
    trailers = release_policy.parse_trailers(str(metadata.get("message", "")))
    source_sha = trailers.get("Source-SHA", "")
    release_policy.validate_sha(source_sha, "release source SHA")
    if parents != [source_sha]:
        raise PreparationError("release identity must have its frozen main commit as its only parent")
    files = sorted(str(item.get("filename", "")) for item in commit.get("files", []))
    if files != ["VERSION"]:
        raise PreparationError("release identity commit must change VERSION only")
    expected = {
        "Source-SHA": source_sha,
        "Product-Version": decision["version"],
        "Release-Baseline-Version": baseline,
        "Release-Intent": decision["version_input"],
    }
    if trailers != expected:
        raise PreparationError("release identity provenance does not match this version decision")
    if version_at_ref(api_root, token, repository, identity_sha) != decision["version"]:
        raise PreparationError("release identity VERSION does not match its provenance")
    if version_at_ref(api_root, token, repository, source_sha) != baseline:
        raise PreparationError("release identity source VERSION does not match its frozen baseline")
    owner, name = repository_parts(repository)
    compare = api_request(
        api_root, token, "GET",
        f"/repos/{owner}/{name}/compare/{source_sha}...{main_sha}",
    )
    if not isinstance(compare, dict) or compare.get("status") not in {"ahead", "identical"}:
        raise PreparationError("release identity source is not an ancestor of the current main commit")
    return {"identity_sha": identity_sha, "parent_sha": source_sha, "trailers": trailers}


def create_identity_commit(
    api_root: str, token: str, repository: str, branch: str, source_sha: str,
    decision: dict[str, str],
) -> str:
    body = "\n".join(
        [
            "Prepare Dockrev version identity",
            "",
            f"Source-SHA: {source_sha}",
            f"Product-Version: {decision['version']}",
            f"Release-Baseline-Version: {decision['baseline_version']}",
            f"Release-Intent: {decision['version_input']}",
        ]
    )
    query = """
    mutation($input: CreateCommitOnBranchInput!) {
      createCommitOnBranch(input: $input) { commit { oid } }
    }
    """
    variables = {
        "input": {
            "branch": {"repositoryNameWithOwner": repository, "branchName": branch},
            "expectedHeadOid": source_sha,
            "message": {"headline": "chore(release): prepare VERSION", "body": body},
            "fileChanges": {
                "additions": [{"path": "VERSION", "contents": decision["version"] + "\n"}]
            },
        }
    }
    data = graphql(api_root, token, query, variables)
    oid = (((data.get("createCommitOnBranch") or {}).get("commit") or {}).get("oid"))
    release_policy.validate_sha(str(oid), "created identity SHA")
    return str(oid)


def reserve_version(
    api_root: str, token: str, repository: str, version: str, identity_sha: str,
) -> None:
    owner, name = repository_parts(repository)
    branch = f"release-reservation/v{version}"
    live = branch_ref(api_root, token, repository, branch)
    if live is not None:
        if live != identity_sha:
            raise PreparationError("target version is reserved by another immutable identity")
        return
    response = api_request(
        api_root, token, "POST", f"/repos/{owner}/{name}/git/refs",
        {"ref": f"refs/heads/{branch}", "sha": identity_sha},
    )
    if response is None and branch_ref(api_root, token, repository, branch) != identity_sha:
        raise PreparationError("target version reservation raced with a foreign identity")
    if response is not None and response.get("object", {}).get("sha") != identity_sha:
        raise PreparationError("target version reservation points at an unexpected identity")


def find_or_create_pull_request(
    api_root: str, token: str, repository: str, branch: str, version: str,
    identity_sha: str,
) -> dict[str, Any]:
    owner, name = repository_parts(repository)
    query = urllib.parse.urlencode({"state": "open", "head": f"{owner}:{branch}", "base": "main", "per_page": 100})
    pulls = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/pulls?{query}")
    if not isinstance(pulls, list):
        raise PreparationError("GitHub did not return the release identity PR list")
    if len(pulls) > 1:
        raise PreparationError("multiple open PRs exist for one release identity branch")
    if pulls:
        pull = pulls[0]
    else:
        pull = api_request(
            api_root, token, "POST", f"/repos/{owner}/{name}/pulls",
            {
                "title": f"chore(release): prepare {version}",
                "head": branch,
                "base": "main",
                "body": (
                    f"Prepare signed VERSION identity `{version}` from the latest `main:VERSION`.\n\n"
                    "The Release completion check verifies the frozen baseline, signature, identity, and reservation."
                ),
            },
        )
    if pull.get("state") != "open" or pull.get("base", {}).get("ref") != "main":
        raise PreparationError("release identity PR is not open against main")
    head = pull.get("head") or {}
    head_repository = (head.get("repo") or {}).get("full_name")
    if head_repository != repository or head.get("sha") != identity_sha:
        raise PreparationError("release identity PR head does not match the reserved identity")
    return pull


def prepare(args: argparse.Namespace) -> dict[str, Any]:
    release_policy.load_policy()
    source_sha = main_sha(args.api_root, args.token, args.repository)
    baseline = version_at_ref(args.api_root, args.token, args.repository, source_sha)
    decision = release_policy.compute_target(baseline, args.version)
    branch = f"release-preparation/v{decision['version']}"
    assert_no_competing_release_preparation(args.api_root, args.token, args.repository, branch)

    live_branch_sha = branch_ref(args.api_root, args.token, args.repository, branch)
    if live_branch_sha is None:
        live_branch_sha = create_branch(args.api_root, args.token, args.repository, branch, source_sha)
    if live_branch_sha == source_sha:
        identity_sha = create_identity_commit(
            args.api_root, args.token, args.repository, branch, source_sha, decision
        )
    else:
        identity_sha = live_branch_sha
    identity = inspect_identity(
        args.api_root, args.token, args.repository, identity_sha,
        source_sha, baseline, decision,
    )
    assert_baseline_current(args.api_root, args.token, args.repository, baseline)
    assert_no_competing_release_preparation(args.api_root, args.token, args.repository, branch)
    reserve_version(args.api_root, args.token, args.repository, decision["version"], identity_sha)
    pull = find_or_create_pull_request(
        args.api_root, args.token, args.repository, branch, decision["version"], identity_sha
    )
    return {
        "schema_version": 1,
        "baseline_sha": identity["parent_sha"],
        "main_sha": source_sha,
        **decision,
        "identity_sha": identity_sha,
        "release_tag": f"v{decision['version']}",
        "pull_request": int(pull["number"]),
        "pull_request_url": pull.get("html_url", ""),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--token", default="")
    parser.add_argument("--api-root", default="https://api.github.com")
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if not args.token:
        parser.error("--token or GITHUB_TOKEN is required")
    try:
        result = prepare(args)
        rendered = json.dumps(result, sort_keys=True, indent=2) + "\n"
        if args.output:
            args.output.write_text(rendered, encoding="utf-8")
        else:
            print(rendered, end="")
    except (PreparationError, release_policy.PolicyError) as error:
        print(str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
