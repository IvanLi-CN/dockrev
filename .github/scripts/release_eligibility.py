#!/usr/bin/env python3
"""Verify a merged release identity satisfied the declared publication gate."""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

import release_policy

ROOT = Path(__file__).resolve().parents[2]
DECLARATION = ROOT / ".github/quality-gates.json"


class EligibilityError(RuntimeError):
    pass


def api_json(api_root: str, token: str, path: str) -> Any:
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}{path}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "dockrev-release-eligibility",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.loads(response.read().decode(response.headers.get_content_charset() or "utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        raise EligibilityError(f"GitHub API request failed: {error.code}: {detail[:400]}") from error
    except urllib.error.URLError as error:
        raise EligibilityError(f"GitHub API request failed: {error.reason}") from error


def load_declaration(path: Path = DECLARATION) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise EligibilityError(f"quality-gates declaration is unavailable: {error}") from error
    if not isinstance(payload, dict):
        raise EligibilityError("quality-gates declaration must be a JSON object")
    return payload


def enforced_contexts(declaration: dict[str, Any]) -> list[str]:
    required = declaration.get("required_checks", [])
    if not isinstance(required, list) or not all(isinstance(item, str) and item for item in required):
        raise EligibilityError("required_checks must be a list of non-empty strings")
    pending = declaration.get("pending_required_checks", [])
    if not isinstance(pending, list):
        raise EligibilityError("pending_required_checks must be a list")
    return sorted(set(required) - set(pending))


def merge_identity_binding(merge: dict[str, Any], identity_sha: str) -> None:
    release_policy.validate_sha(identity_sha, "identity_sha")
    parents = [str(item.get("sha", "")) for item in merge.get("parents", [])]
    if identity_sha not in parents:
        raise EligibilityError("release identity is not a parent of the merged commit")


def commit_check_runs(api_root: str, token: str, repository: str, commit_sha: str) -> list[dict[str, Any]]:
    owner, separator, name = repository.partition("/")
    if not separator or not owner or not name:
        raise EligibilityError("repository must be owner/name")
    quoted = urllib.parse.quote(commit_sha, safe="")
    runs: list[dict[str, Any]] = []
    page = 1
    while True:
        payload = api_json(
            api_root, token,
            f"/repos/{owner}/{name}/commits/{quoted}/check-runs?filter=latest&per_page=100&page={page}",
        )
        if not isinstance(payload, dict) or not isinstance(payload.get("check_runs"), list):
            raise EligibilityError("GitHub did not return check runs for the release identity commit")
        batch = payload["check_runs"]
        runs.extend(item for item in batch if isinstance(item, dict))
        if len(batch) < 100:
            return runs
        page += 1


def verify_publication_gate(
    declaration: dict[str, Any], runs: list[dict[str, Any]], identity_sha: str,
) -> dict[str, Any]:
    release_policy.validate_sha(identity_sha, "identity_sha")
    contexts = enforced_contexts(declaration)
    latest: dict[str, dict[str, Any]] = {}
    for run in runs:
        name = run.get("name")
        if not isinstance(name, str) or name not in contexts:
            continue
        if run.get("head_sha") != identity_sha:
            raise EligibilityError("check run belongs to a different commit than the release identity")
        latest[name] = run
    missing = sorted(set(contexts) - set(latest))
    if missing:
        raise EligibilityError(f"release identity is missing required checks: {', '.join(missing)}")
    failed = sorted(
        f"{name}={latest[name].get('conclusion')}"
        for name in contexts
        if latest[name].get("conclusion") != "success"
    )
    if failed:
        raise EligibilityError(f"release identity did not pass required checks: {', '.join(failed)}")
    return {
        "identity_sha": identity_sha,
        "required_checks": contexts,
        "check_runs": [
            {"name": name, "conclusion": latest[name].get("conclusion"), "id": latest[name].get("id")}
            for name in contexts
        ],
    }


def evaluate(
    api_root: str, token: str, repository: str, merge_sha: str, identity_sha: str,
    declaration: dict[str, Any],
) -> dict[str, Any]:
    owner, separator, name = repository.partition("/")
    if not separator or not owner or not name:
        raise EligibilityError("repository must be owner/name")
    release_policy.validate_sha(merge_sha, "merge_sha")
    merge = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{urllib.parse.quote(merge_sha, safe='')}")
    if not isinstance(merge, dict):
        raise EligibilityError("merged commit is unavailable")
    merge_identity_binding(merge, identity_sha)
    runs = commit_check_runs(api_root, token, repository, identity_sha)
    evidence = verify_publication_gate(declaration, runs, identity_sha)
    return {"merge_commit_sha": merge_sha, **evidence}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--merge-sha", required=True)
    parser.add_argument("--identity-sha", required=True)
    parser.add_argument("--token", default="")
    parser.add_argument("--api-root", default="https://api.github.com")
    parser.add_argument("--declaration", type=Path, default=DECLARATION)
    parser.add_argument("--output", type=argparse.FileType("w", encoding="utf-8"), default=sys.stdout)
    args = parser.parse_args()
    if not args.token:
        parser.error("--token is required")
    try:
        declaration = load_declaration(args.declaration)
        result = evaluate(
            args.api_root, args.token, args.repository,
            args.merge_sha, args.identity_sha, declaration,
        )
    except (EligibilityError, release_policy.PolicyError) as error:
        print(str(error), file=sys.stderr)
        return 2
    json.dump(result, args.output, indent=2, sort_keys=True)
    args.output.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
