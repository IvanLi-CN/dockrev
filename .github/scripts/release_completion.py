#!/usr/bin/env python3
"""Validate a protected VERSION-only manual release identity PR."""

from __future__ import annotations

import argparse
import base64
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
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
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.loads(response.read().decode(response.headers.get_content_charset() or "utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        if error.code == 404:
            return None
        raise CompletionError(f"GitHub API request failed: {error.code}: {detail[:400]}") from error
    except urllib.error.URLError as error:
        raise CompletionError(f"GitHub API request failed: {error.reason}") from error


def repository_parts(repository: str) -> tuple[str, str]:
    owner, separator, name = repository.partition("/")
    if not separator or not owner or not name:
        raise CompletionError("repository must be owner/name")
    return owner, name


def version_at(api_root: str, token: str, repository: str, ref: str) -> str:
    owner, name = repository_parts(repository)
    quoted = urllib.parse.quote(ref, safe="")
    payload = api_json(api_root, token, f"/repos/{owner}/{name}/contents/VERSION?ref={quoted}")
    if not isinstance(payload, dict) or payload.get("encoding") != "base64":
        raise CompletionError(f"{ref}:VERSION is missing or invalid")
    try:
        encoded = "".join(str(payload["content"]).split())
        version = release_policy.parse_version_file(
            base64.b64decode(encoded, validate=True).decode()
        )
        return version
    except (KeyError, ValueError, UnicodeDecodeError, release_policy.PolicyError) as error:
        raise CompletionError(f"{ref}:VERSION is invalid") from error


def changed_files(api_root: str, token: str, repository: str, pull_number: int) -> list[str]:
    owner, name = repository_parts(repository)
    files: list[str] = []
    page = 1
    while True:
        result = api_json(
            api_root, token,
            f"/repos/{owner}/{name}/pulls/{pull_number}/files?per_page=100&page={page}",
        )
        if not isinstance(result, list):
            raise CompletionError("GitHub did not return the release PR changed files")
        files.extend(str(item.get("filename", "")) for item in result)
        if len(result) < 100:
            return sorted(set(files))
        page += 1


def branch_sha(api_root: str, token: str, repository: str, branch: str) -> str | None:
    owner, name = repository_parts(repository)
    quoted = urllib.parse.quote(branch, safe="/")
    result = api_json(api_root, token, f"/repos/{owner}/{name}/git/ref/heads/{quoted}")
    if result is None:
        return None
    sha = str((result.get("object") or {}).get("sha", ""))
    release_policy.validate_sha(sha, "release ref SHA")
    return sha


def validate_completion(payload: dict[str, Any]) -> dict[str, Any]:
    required = {
        "repository", "pull_request", "head_sha", "source_sha", "baseline_version",
        "version_input", "version", "channel", "verified", "parents", "changed_files",
        "main_version", "reservation_sha", "identity_sha",
    }
    missing = sorted(required - set(payload))
    if missing:
        raise CompletionError(f"completion evidence missing: {', '.join(missing)}")
    for field in ("head_sha", "source_sha", "identity_sha", "reservation_sha"):
        release_policy.validate_sha(str(payload[field]), field)
    if payload["verified"] is not True:
        raise CompletionError("release identity commit is not signed")
    if payload["parents"] != [payload["source_sha"]]:
        raise CompletionError("release identity must have exactly its frozen main commit as parent")
    if payload["changed_files"] != ["VERSION"]:
        raise CompletionError("release identity PR must change VERSION only")
    if payload["identity_sha"] != payload["head_sha"] or payload["reservation_sha"] != payload["head_sha"]:
        raise CompletionError("release identity and immutable version reservation do not match")
    if payload["main_version"] != payload["baseline_version"]:
        raise CompletionError("main:VERSION changed after this release identity was prepared")
    try:
        decision = release_policy.compute_target(
            str(payload["baseline_version"]), str(payload["version_input"])
        )
    except release_policy.PolicyError as error:
        raise CompletionError(str(error)) from error
    if decision["version"] != payload["version"] or decision["channel"] != payload["channel"]:
        raise CompletionError("release identity does not match its frozen version decision")
    return payload


def validate_nonrelease_completion(payload: dict[str, Any]) -> dict[str, Any]:
    required = {"repository", "pull_request", "head_sha", "changed_files", "trailers"}
    missing = sorted(required - set(payload))
    if missing:
        raise CompletionError(f"non-release completion evidence missing: {', '.join(missing)}")
    release_policy.validate_sha(str(payload["head_sha"]), "head_sha")
    if payload["trailers"]:
        raise CompletionError("partial release identity provenance must fail closed")
    if "VERSION" in payload["changed_files"]:
        raise CompletionError("VERSION changes require a signed release identity")
    return {
        "repository": payload["repository"],
        "pull_request": payload["pull_request"],
        "head_sha": payload["head_sha"],
        "changed_files": payload["changed_files"],
        "status": "not-applicable",
        "release_enabled": False,
        "reason": "no-release-identity",
    }


def load_github_completion(
    api_root: str, token: str, repository: str, pull_number: int, head_sha: str,
) -> dict[str, Any]:
    owner, name = repository_parts(repository)
    pull = api_json(api_root, token, f"/repos/{owner}/{name}/pulls/{pull_number}")
    if not isinstance(pull, dict) or pull.get("state") != "open":
        raise CompletionError("release identity PR must be open")
    if pull.get("base", {}).get("ref") != "main":
        raise CompletionError("release identity PR must target main")
    if (pull.get("base", {}).get("repo") or {}).get("full_name", "").casefold() != repository.casefold():
        raise CompletionError("release identity PR base repository does not match this repository")
    if (pull.get("head", {}).get("repo") or {}).get("full_name", "").casefold() != repository.casefold():
        raise CompletionError("release identity PR must use a same-repository branch")
    if pull.get("head", {}).get("sha") != head_sha:
        raise CompletionError("release identity PR head changed during completion validation")
    branch = str(pull.get("head", {}).get("ref", ""))
    commit = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{head_sha}")
    if not isinstance(commit, dict):
        raise CompletionError("release identity commit is unavailable")
    metadata = commit.get("commit") or {}
    parents = [str(parent.get("sha", "")) for parent in commit.get("parents", [])]
    files = sorted(str(item.get("filename", "")) for item in commit.get("files", []))
    try:
        trailers = release_policy.parse_trailers(str(metadata.get("message", "")))
    except release_policy.PolicyError as error:
        raise CompletionError(str(error)) from error
    pull_files = changed_files(api_root, token, repository, pull_number)
    if not trailers:
        return validate_nonrelease_completion({
            "repository": repository,
            "pull_request": int(pull_number),
            "head_sha": head_sha,
            "changed_files": pull_files,
            "trailers": trailers,
        })
    source_sha = trailers.get("Source-SHA", "")
    release_policy.validate_sha(source_sha, "release source SHA")
    if parents != [source_sha]:
        raise CompletionError("release identity parent does not match its signed source SHA")
    if files != ["VERSION"] or pull_files != ["VERSION"]:
        raise CompletionError("release identity PR must change VERSION only")
    baseline = trailers.get("Release-Baseline-Version", "")
    version_input = trailers.get("Release-Intent", "")
    target = trailers.get("Product-Version", "")
    source_version = version_at(api_root, token, repository, source_sha)
    if source_version != baseline:
        raise CompletionError("frozen source VERSION does not match the recorded baseline")
    if version_at(api_root, token, repository, head_sha) != target:
        raise CompletionError("signed release identity VERSION does not match its provenance")
    main_version = version_at(api_root, token, repository, "main")
    if main_version != baseline:
        raise CompletionError("main:VERSION changed after this release identity was prepared")
    compare = api_json(
        api_root, token,
        f"/repos/{owner}/{name}/compare/{urllib.parse.quote(source_sha, safe='')}...main",
    )
    if not isinstance(compare, dict) or compare.get("status") not in {"ahead", "identical"}:
        raise CompletionError("release identity source is not an ancestor of main")
    try:
        decision = release_policy.compute_target(baseline, version_input)
    except release_policy.PolicyError as error:
        raise CompletionError(str(error)) from error
    if decision["version"] != target:
        raise CompletionError("signed release identity does not match its version decision")
    if branch != f"release-preparation/v{target}":
        raise CompletionError("release identity branch does not match its immutable target")
    reservation_sha = branch_sha(api_root, token, repository, f"release-reservation/v{target}")
    if reservation_sha != head_sha:
        raise CompletionError("immutable version reservation does not point to this identity")
    lock_sha = branch_sha(api_root, token, repository, f"release-publication-lock/v{target}")
    if lock_sha not in {None, head_sha}:
        raise CompletionError("target version publication lock belongs to another identity")
    for tag in (f"v{target}", target):
        encoded = urllib.parse.quote(tag, safe="")
        existing = api_json(api_root, token, f"/repos/{owner}/{name}/git/ref/tags/{encoded}")
        if existing is not None:
            raise CompletionError("target release tag already exists")
    result = {
        "repository": repository,
        "pull_request": int(pull_number),
        "head_sha": head_sha,
        "identity_sha": head_sha,
        "source_sha": source_sha,
        "baseline_version": baseline,
        "version_input": version_input,
        "version": target,
        "channel": decision["channel"],
        "verified": (metadata.get("verification") or {}).get("verified") is True,
        "parents": parents,
        "changed_files": ["VERSION"],
        "main_version": main_version,
        "reservation_sha": reservation_sha,
    }
    validate_completion(result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--pull-number", type=int, required=True)
    parser.add_argument("--head-sha", required=True)
    parser.add_argument("--token", default="")
    parser.add_argument("--api-root", default="https://api.github.com")
    args = parser.parse_args()
    token = args.token or __import__("os").environ.get("GITHUB_TOKEN", "")
    if not token:
        parser.error("--token or GITHUB_TOKEN is required")
    try:
        release_policy.load_policy()
        release_policy.validate_sha(args.head_sha, "head_sha")
        result = load_github_completion(args.api_root, token, args.repository, args.pull_number, args.head_sha)
        print(json.dumps(result, sort_keys=True, indent=2))
    except (CompletionError, release_policy.PolicyError) as error:
        print(str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
