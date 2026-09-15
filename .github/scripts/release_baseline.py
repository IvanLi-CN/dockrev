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
    """Return true only for a real HTTP 404 in the exception chain."""
    seen: set[int] = set()
    current: BaseException | None = error
    while current is not None and id(current) not in seen:
        seen.add(id(current))
        if isinstance(current, urllib.error.HTTPError) and current.code == 404:
            return True
        if current.__cause__ is not None:
            current = current.__cause__
        elif current.__suppress_context__:
            current = None
        else:
            current = current.__context__
    return False


def _optional_fetch(fetch: Fetch, path: str) -> Any | None:
    try:
        payload = fetch(path)
    except Exception as error:
        if _is_not_found(error):
            return None
        raise
    if payload is None:
        raise BaselineError("GitHub API response is invalid")
    return payload


def release_metadata(release: Any) -> tuple[str, bool, bool, str]:
    if not isinstance(release, dict):
        raise BaselineError("GitHub release is invalid")
    tag = release.get("tag_name")
    draft = release.get("draft")
    prerelease = release.get("prerelease")
    author = release.get("author")
    if not isinstance(tag, str) or not tag:
        raise BaselineError("GitHub release tag_name is invalid")
    if not isinstance(draft, bool) or not isinstance(prerelease, bool):
        raise BaselineError("GitHub release draft state is invalid")
    if not isinstance(author, dict):
        raise BaselineError("GitHub release author is invalid")
    author_login = author.get("login")
    if not isinstance(author_login, str) or not author_login:
        raise BaselineError("GitHub release author login is invalid")
    return tag, draft, prerelease, author_login


def tag_target(target: Any, description: str) -> tuple[str, str]:
    if not isinstance(target, dict):
        raise BaselineError(f"{description} is invalid")
    target_type = target.get("type")
    target_sha = target.get("sha")
    if target_type not in {"commit", "tag"}:
        raise BaselineError(f"{description} type is invalid")
    if not isinstance(target_sha, str):
        raise BaselineError(f"{description} SHA is invalid")
    try:
        release_policy.validate_sha(target_sha, f"{description} SHA")
    except release_policy.PolicyError as error:
        raise BaselineError(f"{description} SHA is invalid") from error
    return target_type, target_sha


def tagged_commit_sha(fetch: Fetch, repository: str, tag: str) -> str | None:
    owner, name = repository_parts(repository)
    ref = _optional_fetch(
        fetch,
        f"/repos/{owner}/{name}/git/ref/tags/{urllib.parse.quote(tag, safe='')}",
    )
    if ref is None:
        return None
    if not isinstance(ref, dict):
        raise BaselineError("GitHub tag ref is invalid")
    target = ref.get("object")
    for _ in range(5):
        target_type, target_sha = tag_target(target, "GitHub tag target")
        if target_type == "commit":
            return target_sha
        annotated = _optional_fetch(fetch, f"/repos/{owner}/{name}/git/tags/{target_sha}")
        if annotated is None:
            return None
        if not isinstance(annotated, dict):
            raise BaselineError("GitHub annotated tag is invalid")
        target = annotated.get("object")
    raise BaselineError("GitHub tag annotation depth exceeds the supported limit")


def is_main_reachable(fetch: Fetch, repository: str, commit_sha: str) -> bool:
    owner, name = repository_parts(repository)
    comparison = _optional_fetch(
        fetch,
        f"/repos/{owner}/{name}/compare/{urllib.parse.quote(f'{commit_sha}...main', safe='')}",
    )
    if comparison is None:
        return False
    if not isinstance(comparison, dict):
        raise BaselineError("GitHub comparison is invalid")
    status = comparison.get("status")
    if status not in {"ahead", "behind", "diverged", "identical"}:
        raise BaselineError("GitHub comparison status is invalid")
    return status in {"ahead", "identical"}


def is_qualified_final_release(fetch: Fetch, repository: str, release: Any) -> bool:
    tag, draft, prerelease, author_login = release_metadata(release)
    if draft or prerelease:
        return False
    if author_login != RELEASE_AUTOMATION_ACTOR:
        return False
    if final_version_from_tag(tag) is None:
        return False
    target_sha = tagged_commit_sha(fetch, repository, tag)
    return target_sha is not None and is_main_reachable(fetch, repository, target_sha)


def release_for_tag(fetch: Fetch, repository: str, tag: str) -> dict[str, Any] | None:
    owner, name = repository_parts(repository)
    release = _optional_fetch(
        fetch,
        f"/repos/{owner}/{name}/releases/tags/{urllib.parse.quote(tag, safe='')}",
    )
    if release is None:
        return None
    returned_tag, _draft, _prerelease, _author_login = release_metadata(release)
    if returned_tag != tag:
        raise BaselineError("GitHub release tag does not match the requested tag")
    return release


def latest_qualified_final_release_version(fetch: Fetch, repository: str) -> str:
    owner, name = repository_parts(repository)
    candidates: list[tuple[tuple[int, int, int], dict[str, Any], str]] = []
    page = 1
    while True:
        releases = fetch(f"/repos/{owner}/{name}/releases?per_page=100&page={page}")
        if not isinstance(releases, list):
            raise BaselineError("GitHub release list is invalid")
        for release in releases:
            tag, _draft, _prerelease, _author_login = release_metadata(release)
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
