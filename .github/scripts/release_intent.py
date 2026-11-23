#!/usr/bin/env python3
"""Resolve a manual release intent against the trusted final release baseline."""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

import release_baseline
import release_policy


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


def fetch(api_root: str, token: str):
    return lambda path: api_json(api_root, token, path)


def repository_parts(repository: str) -> tuple[str, str]:
    owner, separator, name = repository.partition("/")
    if not separator or not owner or not name:
        raise IntentError("repository must be owner/name")
    return owner, name


def ci_success(api_root: str, token: str, repository: str, commit_sha: str, workflow_path: str) -> None:
    owner, name = repository_parts(repository)
    quoted = urllib.parse.quote(commit_sha, safe="")
    payload = api_json(
        api_root, token,
        f"/repos/{owner}/{name}/actions/workflows/{urllib.parse.quote(workflow_path, safe='')}"
        f"/runs?head_sha={quoted}&status=completed&per_page=100",
    )
    runs = (payload or {}).get("workflow_runs")
    if not isinstance(runs, list):
        raise IntentError(f"GitHub did not return runs for {workflow_path}")
    if not any(run.get("conclusion") == "success" for run in runs if isinstance(run, dict)):
        raise IntentError(f"{workflow_path} has no successful completed run for {commit_sha}")


def baseline_ci_ok(
    api_root: str, token: str, repository: str, commit_sha: str, workflow_path: str,
) -> None:
    """Legacy final releases predate the main CI workflow and only supply the baseline."""
    owner, name = repository_parts(repository)
    quoted = urllib.parse.quote(commit_sha, safe="")
    payload = api_json(
        api_root, token,
        f"/repos/{owner}/{name}/actions/workflows/{urllib.parse.quote(workflow_path, safe='')}"
        f"/runs?head_sha={quoted}&status=completed&per_page=100",
    )
    runs = (payload or {}).get("workflow_runs")
    if not isinstance(runs, list):
        raise IntentError(f"GitHub did not return runs for {workflow_path}")
    completed = [run for run in runs if isinstance(run, dict)]
    if not completed or any(run.get("conclusion") == "success" for run in completed):
        return
    raise IntentError(f"baseline {commit_sha} has failing {workflow_path} runs")


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
    commit = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{source_sha}")
    if not isinstance(commit, dict):
        raise IntentError("dispatch source commit is unavailable")
    if (commit.get("commit", {}).get("verification") or {}).get("verified") is not True:
        raise IntentError(f"dispatch source {source_sha} has no verified commit signature")
    main_ref = api_json(api_root, token, f"/repos/{owner}/{name}/git/ref/heads/main")
    main_sha = str(((main_ref or {}).get("object") or {}).get("sha", ""))
    release_policy.validate_sha(main_sha, "main SHA")
    comparison = api_json(
        api_root, token,
        f"/repos/{owner}/{name}/compare/{urllib.parse.quote(source_sha, safe='')}...{main_sha}?per_page=1",
    )
    if not isinstance(comparison, dict) or comparison.get("status") not in {"ahead", "identical"}:
        raise IntentError(f"dispatch source {source_sha} is not reachable from main")
    ci_success(api_root, token, repository, source_sha, workflow_path)
    return {"target": target, "source_sha": source_sha, "main_sha": main_sha}


def resolve_baseline(api_root: str, token: str, repository: str, workflow_path: str) -> dict[str, Any]:
    api = fetch(api_root, token)
    version = release_baseline.latest_qualified_final_release_version(api, repository)
    if version == "0.0.0":
        raise IntentError("no published final GitHub Release is available as a baseline")
    source_sha = release_baseline.tagged_commit_sha(api, repository, f"v{version}")
    if source_sha is None:
        raise IntentError(f"baseline v{version} has no resolvable commit")
    baseline_ci_ok(api_root, token, repository, source_sha, workflow_path)
    return {"version": version, "tag": f"v{version}", "source_sha": source_sha}


def resolve_range(
    api_root: str, token: str, repository: str, baseline_sha: str, source_sha: str,
) -> dict[str, Any]:
    release_policy.validate_sha(baseline_sha, "baseline SHA")
    release_policy.validate_sha(source_sha, "source SHA")
    owner, name = repository_parts(repository)
    comparison = api_json(
        api_root, token,
        f"/repos/{owner}/{name}/compare/{urllib.parse.quote(baseline_sha, safe='')}"
        f"...{urllib.parse.quote(source_sha, safe='')}?per_page=100",
    )
    if not isinstance(comparison, dict):
        raise IntentError("release range comparison is unavailable")
    if comparison.get("status") != "ahead":
        raise IntentError("release source must be a non-empty descendant of the trusted baseline")
    commits = comparison.get("commits")
    if not isinstance(commits, list) or not commits:
        raise IntentError("release range has no commits")
    total = comparison.get("total_commits")
    if isinstance(total, int) and total > len(commits):
        raise IntentError("release range exceeds the compare API page and cannot be fully verified")
    return {"baseline_sha": baseline_sha, "source_sha": source_sha, "commits": len(commits)}


def resolve_version(baseline_version: str, target: str, tags_path: Path | None) -> dict[str, Any]:
    tags = tags_path.read_text(encoding="utf-8").split() if tags_path else []
    decision = release_policy.compute_target(baseline_version, target, tags)
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
            result = resolve_version(args.baseline_version, args.target, args.tags)
    except (IntentError, release_policy.PolicyError, release_baseline.BaselineError, OSError) as error:
        print(str(error), file=sys.stderr)
        return 2
    rendered = json.dumps(result, sort_keys=True, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
