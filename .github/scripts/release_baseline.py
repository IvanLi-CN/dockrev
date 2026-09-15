#!/usr/bin/env python3
"""Resolve and revalidate qualified final GitHub Release baselines."""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable
from typing import Any

import release_policy


RELEASE_AUTOMATION_ACTOR = "github-actions[bot]"


class BaselineError(ValueError):
    pass


Fetch = Callable[[str], Any]


def repository_parts(repository: str) -> tuple[str, str]:
    try:
        owner, name = repository.split("/", 1)
    except ValueError as error:
        raise BaselineError("repository must be owner/name") from error
    if not owner or not name:
        raise BaselineError("repository must be owner/name")
    return owner, name


def final_version_from_tag(tag: str) -> str | None:
    if not tag.startswith("v"):
        return None
    try:
        major, minor, patch, prerelease = release_policy.parse_version(tag[1:])
    except release_policy.PolicyError:
        return None
    if prerelease is not None:
        return None
    return f"{major}.{minor}.{patch}"


def _is_not_found(error: Exception) -> bool:
    return "404" in str(error)


def _optional_fetch(fetch: Fetch, path: str) -> Any | None:
    try:
        return fetch(path)
    except Exception as error:
        if _is_not_found(error):
            return None
        raise


def tagged_commit_sha(fetch: Fetch, repository: str, tag: str) -> str | None:
    owner, name = repository_parts(repository)
    ref = _optional_fetch(
        fetch,
        f"/repos/{owner}/{name}/git/ref/tags/{urllib.parse.quote(tag, safe='')}",
    )
    if not isinstance(ref, dict):
        return None
    target = ref.get("object")
    for _ in range(5):
        if not isinstance(target, dict):
            return None
        target_type = target.get("type")
        target_sha = target.get("sha")
        if not isinstance(target_sha, str):
            return None
        if target_type == "commit":
            try:
                release_policy.validate_sha(target_sha, "final release tag target SHA")
            except release_policy.PolicyError:
                return None
            return target_sha
        if target_type != "tag":
            return None
        annotated = _optional_fetch(fetch, f"/repos/{owner}/{name}/git/tags/{target_sha}")
        if not isinstance(annotated, dict):
            return None
        target = annotated.get("object")
    return None


def is_main_reachable(fetch: Fetch, repository: str, commit_sha: str) -> bool:
    owner, name = repository_parts(repository)
    comparison = _optional_fetch(
        fetch,
        f"/repos/{owner}/{name}/compare/{urllib.parse.quote(f'{commit_sha}...main', safe='')}",
    )
    return isinstance(comparison, dict) and comparison.get("status") in {"ahead", "identical"}


def is_qualified_final_release(fetch: Fetch, repository: str, release: Any) -> bool:
    if not isinstance(release, dict):
        return False
    if release.get("draft") is not False or release.get("prerelease") is not False:
        return False
    author = release.get("author")
    if not isinstance(author, dict) or author.get("login") != RELEASE_AUTOMATION_ACTOR:
        return False
    tag = release.get("tag_name")
    if not isinstance(tag, str) or final_version_from_tag(tag) is None:
        return False
    target_sha = tagged_commit_sha(fetch, repository, tag)
    return target_sha is not None and is_main_reachable(fetch, repository, target_sha)


def release_for_tag(fetch: Fetch, repository: str, tag: str) -> dict[str, Any] | None:
    owner, name = repository_parts(repository)
    release = _optional_fetch(
        fetch,
        f"/repos/{owner}/{name}/releases/tags/{urllib.parse.quote(tag, safe='')}",
    )
    return release if isinstance(release, dict) else None


def latest_qualified_final_release_version(fetch: Fetch, repository: str) -> str:
    owner, name = repository_parts(repository)
    candidates: list[tuple[tuple[int, int, int], dict[str, Any], str]] = []
    page = 1
    while True:
        releases = fetch(f"/repos/{owner}/{name}/releases?per_page=100&page={page}")
        if not isinstance(releases, list):
            raise BaselineError("GitHub release list is invalid")
        for release in releases:
            if not isinstance(release, dict):
                continue
            tag = release.get("tag_name")
            if not isinstance(tag, str):
                continue
            version = final_version_from_tag(tag)
            if version is None:
                continue
            major, minor, patch, _prerelease = release_policy.parse_version(version)
            candidates.append(((major, minor, patch), release, version))
        if len(releases) < 100:
            break
        page += 1
    for _numeric, release, version in sorted(candidates, key=lambda candidate: candidate[0], reverse=True):
        if is_qualified_final_release(fetch, repository, release):
            return version
    return "0.0.0"


def validate_frozen_final_baseline(fetch: Fetch, repository: str, baseline_version: str) -> None:
    try:
        major, minor, patch, prerelease = release_policy.parse_version(baseline_version)
    except release_policy.PolicyError as error:
        raise BaselineError("release baseline is not valid semver") from error
    if prerelease is not None:
        raise BaselineError("release baseline must be a final semver version")
    normalized = f"{major}.{minor}.{patch}"
    release = release_for_tag(fetch, repository, f"v{normalized}")
    if release is not None and is_qualified_final_release(fetch, repository, release):
        return
    if normalized == "0.0.0" and latest_qualified_final_release_version(fetch, repository) == "0.0.0":
        return
    raise BaselineError(f"frozen release baseline v{normalized} is not a qualified final release")


def api_fetch(api_root: str, token: str, path: str) -> Any:
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}{path}",
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.loads(response.read().decode())
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        raise BaselineError(f"GitHub API GET failed: {error.code}: {detail}") from error
    except urllib.error.URLError as error:
        raise BaselineError(f"GitHub API GET failed: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--token", required=True)
    parser.add_argument("--api-root", default="https://api.github.com")
    args = parser.parse_args()
    try:
        print(
            latest_qualified_final_release_version(
                lambda path: api_fetch(args.api_root, args.token, path), args.repository
            )
        )
        return 0
    except (BaselineError, release_policy.PolicyError, ValueError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
