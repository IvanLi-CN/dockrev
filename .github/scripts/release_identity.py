#!/usr/bin/env python3
"""Resolve a merged VERSION-only identity without reallocating its version."""

from __future__ import annotations

import argparse
import base64
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

import release_policy


class IdentityError(RuntimeError):
    pass


def api_json(api_root: str, token: str, path: str) -> Any:
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}{path}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "dockrev-release-identity",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.loads(response.read().decode(response.headers.get_content_charset() or "utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        if error.code == 404:
            return None
        raise IdentityError(f"GitHub API request failed: {error.code}: {detail[:400]}") from error
    except urllib.error.URLError as error:
        raise IdentityError(f"GitHub API request failed: {error.reason}") from error


def repository_parts(repository: str) -> tuple[str, str]:
    owner, separator, name = repository.partition("/")
    if not separator or not owner or not name:
        raise IdentityError("repository must be owner/name")
    return owner, name


def version_at_commit(api_root: str, token: str, repository: str, commit_sha: str) -> str:
    owner, name = repository_parts(repository)
    ref = urllib.parse.quote(commit_sha, safe="")
    result = api_json(api_root, token, f"/repos/{owner}/{name}/contents/VERSION?ref={ref}")
    if not isinstance(result, dict) or result.get("encoding") != "base64":
        raise IdentityError(f"VERSION is missing at {commit_sha}")
    try:
        encoded = "".join(str(result["content"]).split())
        version = release_policy.parse_version_file(
            base64.b64decode(encoded, validate=True).decode()
        )
        return version
    except (KeyError, ValueError, UnicodeDecodeError, release_policy.PolicyError) as error:
        raise IdentityError(f"VERSION is invalid at {commit_sha}") from error


def branch_sha(api_root: str, token: str, repository: str, branch: str) -> str | None:
    owner, name = repository_parts(repository)
    quoted = urllib.parse.quote(branch, safe="/")
    result = api_json(api_root, token, f"/repos/{owner}/{name}/git/ref/heads/{quoted}")
    if result is None:
        return None
    sha = str((result.get("object") or {}).get("sha", ""))
    release_policy.validate_sha(sha, "release reservation SHA")
    return sha


def resolve_from_payload(payload: dict[str, Any]) -> dict[str, Any]:
    if not payload.get("release_enabled"):
        if payload.get("reason") != "no-release-identity":
            raise IdentityError("unreleased commit has no valid no-identity reason")
        return payload
    try:
        release_policy.validate_identity(payload)
    except release_policy.PolicyError as error:
        raise IdentityError(str(error)) from error
    return payload


def merged_pull_request(
    api_root: str, token: str, repository: str, merge_sha: str,
) -> dict[str, Any] | None:
    owner, name = repository_parts(repository)
    result = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{merge_sha}/pulls")
    if not isinstance(result, list):
        raise IdentityError("GitHub did not return merged commit pull request associations")
    matching = [
        pull for pull in result
        if isinstance(pull, dict)
        and pull.get("base", {}).get("ref") == "main"
        and pull.get("state") == "closed"
        and pull.get("merged_at")
        and pull.get("merge_commit_sha") == merge_sha
        and (pull.get("base", {}).get("repo") or {}).get("full_name", "").casefold() == repository.casefold()
    ]
    if not matching:
        return None
    if len(matching) != 1:
        raise IdentityError("merged release commit must map to exactly one main PR")
    return matching[0]


def resolve_github(
    api_root: str, token: str, repository: str, merge_sha: str, run_url: str = "",
) -> dict[str, Any]:
    release_policy.validate_sha(merge_sha, "merge_sha")
    pull = merged_pull_request(api_root, token, repository, merge_sha)
    if pull is None:
        return {"release_enabled": False, "reason": "no-release-identity"}
    head = pull.get("head") or {}
    identity_sha = str(head.get("sha", ""))
    release_policy.validate_sha(identity_sha, "identity_sha")
    owner, name = repository_parts(repository)
    commit = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{identity_sha}")
    if not isinstance(commit, dict):
        raise IdentityError("merged release identity commit is unavailable")
    metadata = commit.get("commit") or {}
    try:
        trailers = release_policy.parse_trailers(str(metadata.get("message", "")))
    except release_policy.PolicyError as error:
        raise IdentityError(str(error)) from error
    if not trailers:
        return {"release_enabled": False, "reason": "no-release-identity"}
    if set(trailers) != release_policy.TRAILER_KEYS:
        raise IdentityError("merged release identity provenance is incomplete")
    if (metadata.get("verification") or {}).get("verified") is not True:
        raise IdentityError("merged release identity commit is not signed")
    parents = [str(item.get("sha", "")) for item in commit.get("parents", [])]
    source_sha = trailers["Source-SHA"]
    release_policy.validate_sha(source_sha, "source_sha")
    if parents != [source_sha]:
        raise IdentityError("merged release identity must have its signed source SHA as its only parent")
    if head.get("repo", {}).get("full_name", "").casefold() != repository.casefold():
        raise IdentityError("merged release identity must come from a same-repository branch")
    if head.get("ref") != f"release-preparation/v{trailers['Product-Version']}":
        raise IdentityError("merged release identity branch does not match its target version")
    files = sorted(str(item.get("filename", "")) for item in commit.get("files", []))
    if files != ["VERSION"]:
        raise IdentityError("merged release identity commit must change VERSION only")

    baseline = trailers["Release-Baseline-Version"]
    version = trailers["Product-Version"]
    version_input = trailers["Release-Intent"]
    if version_at_commit(api_root, token, repository, source_sha) != baseline:
        raise IdentityError("release identity source VERSION does not match its frozen baseline")
    if version_at_commit(api_root, token, repository, merge_sha) != version:
        raise IdentityError("merged VERSION does not match its signed release identity")
    try:
        decision = release_policy.compute_target(baseline, version_input)
    except release_policy.PolicyError as error:
        raise IdentityError(str(error)) from error
    if decision["version"] != version:
        raise IdentityError("merged release identity does not match its frozen version decision")
    reservation_sha = branch_sha(api_root, token, repository, f"release-reservation/v{version}")
    if reservation_sha != identity_sha:
        raise IdentityError("immutable version reservation does not point to the merged release identity")

    payload = {
        "release_enabled": True,
        "pull_request": int(pull["number"]),
        "source_sha": source_sha,
        "merge_commit_sha": merge_sha,
        "identity_sha": identity_sha,
        "version": version,
        "baseline_version": baseline,
        "version_input": version_input,
        "channel": decision["channel"],
        "release_tag": f"v{version}",
        "run_url": run_url,
    }
    return resolve_from_payload(payload)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--merge-sha", required=True)
    parser.add_argument("--token", default="")
    parser.add_argument("--api-root", default="https://api.github.com")
    parser.add_argument("--run-url", default="")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    token = args.token or os.environ.get("GITHUB_TOKEN", "")
    if not token:
        parser.error("--token or GITHUB_TOKEN is required")
    try:
        release_policy.load_policy()
        payload = resolve_github(args.api_root, token, args.repository, args.merge_sha, args.run_url)
        rendered = json.dumps(payload, sort_keys=True, indent=2) + "\n"
        if args.output:
            args.output.write_text(rendered, encoding="utf-8")
        else:
            print(rendered, end="")
    except (IdentityError, release_policy.PolicyError) as error:
        print(str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
