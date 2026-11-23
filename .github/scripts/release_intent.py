#!/usr/bin/env python3
"""Resolve a manual release intent against the trusted final release baseline."""

from __future__ import annotations

import argparse
import http.client
import json
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

import release_policy

FINAL_TAG_RE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")
RELEASE_AUTOMATION_ACTOR = "github-actions[bot]"


class IntentError(RuntimeError):
    pass


def api_json(api_root: str, token: str, path: str) -> Any:
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}{path}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "dockrev-release-intent",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.loads(response.read().decode(response.headers.get_content_charset() or "utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        if error.code == 404:
            return None
        raise IntentError(f"GitHub API request failed: {error.code}: {detail[:400]}") from error
    except urllib.error.URLError as error:
        raise IntentError(f"GitHub API request failed: {error.reason}") from error


def api_retry(api_root: str, token: str, path: str, attempts: int = 4) -> Any:
    last_error: BaseException | None = None
    for attempt in range(attempts):
        try:
            return api_json(api_root, token, path)
        except (urllib.error.URLError, http.client.HTTPException, ConnectionError, TimeoutError) as error:
            last_error = error
            time.sleep(1.5 * (attempt + 1))
    raise IntentError(f"GitHub API request failed after {attempts} attempts: {last_error}")


def repository_parts(repository: str) -> tuple[str, str]:
    owner, separator, name = repository.partition("/")
    if not separator or not owner or not name:
        raise IntentError("repository must be owner/name")
    return owner, name


def workflow_runs(
    api_root: str, token: str, repository: str, commit_sha: str, workflow_path: str,
) -> list[dict[str, Any]]:
    owner, name = repository_parts(repository)
    quoted = urllib.parse.quote(commit_sha, safe="")
    payload = api_retry(
        api_root, token,
        f"/repos/{owner}/{name}/actions/workflows/{urllib.parse.quote(workflow_path, safe='')}"
        f"/runs?head_sha={quoted}&status=completed&per_page=100",
    )
    runs = (payload or {}).get("workflow_runs") if isinstance(payload, dict) else None
    if not isinstance(runs, list):
        raise IntentError(f"GitHub did not return runs for {workflow_path}")
    return [run for run in runs if isinstance(run, dict)]


def ci_success(
    api_root: str, token: str, repository: str, commit_sha: str, workflow_path: str,
) -> None:
    runs = workflow_runs(api_root, token, repository, commit_sha, workflow_path)
    if not any(run.get("conclusion") == "success" for run in runs):
        raise IntentError(f"{workflow_path} has no successful completed run for {commit_sha}")


def baseline_ci_ok(
    api_root: str, token: str, repository: str, commit_sha: str, workflow_path: str,
) -> None:
    """Legacy final releases predate the main CI workflow and only supply the baseline."""
    runs = workflow_runs(api_root, token, repository, commit_sha, workflow_path)
    if not runs or any(run.get("conclusion") == "success" for run in runs):
        return
    raise IntentError(f"baseline {commit_sha} has failing {workflow_path} runs")


def main_reachable(api_root: str, token: str, repository: str, commit_sha: str) -> bool:
    owner, name = repository_parts(repository)
    comparison = api_retry(
        api_root, token,
        f"/repos/{owner}/{name}/compare/{urllib.parse.quote(commit_sha, safe='')}...main?per_page=1",
    )
    return isinstance(comparison, dict) and comparison.get("status") in {"ahead", "identical"}


def tagged_commit_sha(api_root: str, token: str, repository: str, tag: str) -> str | None:
    owner, name = repository_parts(repository)
    ref = api_retry(
        api_root, token,
        f"/repos/{owner}/{name}/git/ref/tags/{urllib.parse.quote(tag, safe='')}",
    )
    if not isinstance(ref, dict):
        return None
    target = ref.get("object") or {}
    for _ in range(6):
        object_type = target.get("type")
        object_sha = str(target.get("sha", ""))
        if object_type == "commit":
            return object_sha
        if object_type != "tag":
            return None
        annotated = api_retry(api_root, token, f"/repos/{owner}/{name}/git/tags/{object_sha}")
        if not isinstance(annotated, dict):
            return None
        target = annotated.get("object") or {}
    return None


def resolve_intent(
    api_root: str, token: str, repository: str, source_sha: str, ref: str, target: str,
    workflow_path: str,
) -> dict[str, Any]:
    if ref != "refs/heads/main":
        raise IntentError(f"Release must be dispatched from main, got {ref}")
    release_policy.validate_sha(source_sha, "dispatch source")
    if not target or target != target.strip():
        raise IntentError("target must be a non-empty canonical value")
    owner, name = repository_parts(repository)
    commit = api_retry(api_root, token, f"/repos/{owner}/{name}/commits/{source_sha}")
    if not isinstance(commit, dict):
        raise IntentError("dispatch source commit is unavailable")
    if (commit.get("commit", {}).get("verification") or {}).get("verified") is not True:
        raise IntentError(f"dispatch source {source_sha} has no verified commit signature")
    if not main_reachable(api_root, token, repository, source_sha):
        raise IntentError(f"dispatch source {source_sha} is not reachable from main")
    ci_success(api_root, token, repository, source_sha, workflow_path)
    return {"target": target, "source_sha": source_sha}


def resolve_baseline(api_root: str, token: str, repository: str, workflow_path: str) -> dict[str, Any]:
    owner, name = repository_parts(repository)
    candidates: list[tuple[tuple[int, int, int], str]] = []
    for page in (1, 2):
        releases = api_retry(api_root, token, f"/repos/{owner}/{name}/releases?per_page=100&page={page}")
        if not isinstance(releases, list):
            raise IntentError("GitHub release list is invalid")
        for release in releases:
            if not isinstance(release, dict):
                continue
            if release.get("draft") or release.get("prerelease"):
                continue
            tag = str(release.get("tag_name", ""))
            match = FINAL_TAG_RE.fullmatch(tag)
            if match is None:
                continue
            if str((release.get("author") or {}).get("login", "")) != RELEASE_AUTOMATION_ACTOR:
                continue
            candidates.append((tuple(int(part) for part in match.groups()), tag))
        if candidates or len(releases) < 100:
            break
    for _numeric, tag in sorted(candidates, reverse=True):
        source_sha = tagged_commit_sha(api_root, token, repository, tag)
        if source_sha is None:
            continue
        if not main_reachable(api_root, token, repository, source_sha):
            continue
        baseline_ci_ok(api_root, token, repository, source_sha, workflow_path)
        return {"version": tag[1:], "tag": tag, "source_sha": source_sha}
    raise IntentError("no published final GitHub Release is available as a baseline")


def resolve_range(
    api_root: str, token: str, repository: str, baseline_sha: str, source_sha: str,
) -> dict[str, Any]:
    release_policy.validate_sha(baseline_sha, "baseline SHA")
    release_policy.validate_sha(source_sha, "source SHA")
    owner, name = repository_parts(repository)
    base = f"/repos/{owner}/{name}/compare/{urllib.parse.quote(baseline_sha, safe='')}"
    source = urllib.parse.quote(source_sha, safe="")
    seen: dict[str, None] = {}
    page = 1
    while True:
        comparison = api_retry(api_root, token, f"{base}...{source}?per_page=100&page={page}")
        if not isinstance(comparison, dict):
            raise IntentError("release range comparison is unavailable")
        if comparison.get("status") != "ahead":
            raise IntentError("release source must be a non-empty descendant of the trusted baseline")
        commits = comparison.get("commits")
        if not isinstance(commits, list):
            raise IntentError("release range comparison is invalid")
        for commit in commits:
            sha = str((commit or {}).get("sha", "")) if isinstance(commit, dict) else ""
            release_policy.validate_sha(sha, "release range commit")
            seen[sha] = None
        total = comparison.get("total_commits")
        if isinstance(total, int):
            if len(seen) >= total:
                break
            if not commits:
                raise IntentError("release range comparison returned an empty page before the reported total")
        elif len(commits) < 100:
            break
        page += 1
        if page > 50:
            raise IntentError("release range exceeds 5000 commits and cannot be fully verified")
    if not seen:
        raise IntentError("release range has no commits")
    return {"baseline_sha": baseline_sha, "source_sha": source_sha, "commits": len(seen)}


def parse_tag_lines(path: Path) -> tuple[list[str], dict[str, str]]:
    """Read `<tag> <object> [<peeled>]` lines; a bare tag name has no known SHA."""
    names: list[str] = []
    shas: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if not parts:
            continue
        name = parts[0]
        names.append(name)
        if len(parts) >= 2:
            commit_sha = parts[2] if len(parts) >= 3 else parts[1]
            if len(commit_sha) == 40:
                shas[name] = commit_sha
    return names, shas


def resolve_version(
    baseline_version: str, target: str, tags_path: Path | None, source_sha: str = "",
) -> dict[str, Any]:
    if tags_path is None:
        names: list[str] = []
        shas: dict[str, str] = {}
    else:
        names, shas = parse_tag_lines(tags_path)
    if source_sha:
        release_policy.validate_sha(source_sha, "source SHA")
        names = release_policy.reservation_tags(names, source_sha, shas)
    decision = release_policy.compute_target(baseline_version, target, names)
    return {**decision, "release_tag": f"v{decision['version']}"}


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    intent = subparsers.add_parser("intent")
    baseline = subparsers.add_parser("baseline")
    range_parser = subparsers.add_parser("range")
    resolve = subparsers.add_parser("resolve")

    for item in (intent, baseline, range_parser, resolve):
        item.add_argument("--repository", required=True)
        item.add_argument("--token", default="")
        item.add_argument("--api-root", default="https://api.github.com")
        item.add_argument("--workflow", default="ci-main.yml")
        item.add_argument("--output", type=Path)
    intent.add_argument("--source-sha", required=True)
    intent.add_argument("--ref", required=True)
    intent.add_argument("--target", required=True)
    range_parser.add_argument("--baseline-sha", required=True)
    range_parser.add_argument("--source-sha", required=True)
    resolve.add_argument("--baseline-version", required=True)
    resolve.add_argument("--target", required=True)
    resolve.add_argument("--tags", type=Path, default=None)
    resolve.add_argument("--source-sha", default="")

    args = parser.parse_args()
    if not args.token:
        parser.error("--token is required")
    try:
        release_policy.load_policy()
        if args.command == "intent":
            result = resolve_intent(
                args.api_root, args.token, args.repository, args.source_sha, args.ref,
                args.target, args.workflow,
            )
        elif args.command == "baseline":
            result = resolve_baseline(args.api_root, args.token, args.repository, args.workflow)
        elif args.command == "range":
            result = resolve_range(
                args.api_root, args.token, args.repository, args.baseline_sha, args.source_sha,
            )
        else:
            result = resolve_version(args.baseline_version, args.target, args.tags, args.source_sha)
    except (IntentError, release_policy.PolicyError, OSError) as error:
        print(str(error), file=sys.stderr)
        return 2
    rendered = json.dumps(result, sort_keys=True, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
