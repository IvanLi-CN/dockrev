#!/usr/bin/env python3
"""Select an immutable artifact bundle for same-identity release recovery."""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from typing import Any, Callable


class RecoveryArtifactError(ValueError):
    pass


def select_reusable_artifact(
    runs: list[dict[str, Any]], identity_artifacts: list[dict[str, Any]],
    bundle_artifacts: list[dict[str, Any]], merge_sha: str,
) -> dict[str, Any]:
    run_by_id = {str(run.get("id", "")): run for run in runs}
    identity_name = f"release-intent-{merge_sha}"
    bundle_name = f"release-bundle-{merge_sha}"
    identity_run_ids = {
        str((item.get("workflow_run") or {}).get("id", ""))
        for item in identity_artifacts
        if item.get("name") == identity_name and item.get("expired") is not True
    }
    candidates = []
    for run_id in identity_run_ids:
        run = run_by_id.get(run_id)
        if not run or run.get("conclusion") != "failure" or run.get("head_branch") != "main":
            continue
        same_identity_run = (
            run.get("event") == "push" and run.get("head_sha") == merge_sha
        ) or run.get("event") == "workflow_dispatch"
        if same_identity_run:
            candidates.append(run)
    if not candidates:
        raise RecoveryArtifactError("same-SHA recovery requires a prior failed Release run with this immutable identity")
    candidates.sort(key=lambda item: str(item.get("updated_at", "")), reverse=True)
    for run in candidates:
        run_id = str(run.get("id", ""))
        if not re.fullmatch(r"[1-9][0-9]*", run_id):
            raise RecoveryArtifactError("prior Release run identity is invalid")
        matches = [
            item for item in bundle_artifacts
            if item.get("name") == bundle_name
            and str((item.get("workflow_run") or {}).get("id", "")) == run_id
        ]
        if len(matches) > 1:
            raise RecoveryArtifactError("prior Release run has duplicate immutable bundle artifacts")
        if not matches or matches[0].get("expired") is True:
            continue
        artifact = matches[0]
        digest = str(artifact.get("digest", ""))
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
            raise RecoveryArtifactError("prior release bundle is missing its SHA-256 artifact digest")
        artifact_id = str(artifact.get("id", ""))
        if not re.fullmatch(r"[1-9][0-9]*", artifact_id):
            raise RecoveryArtifactError("prior release bundle artifact ID is invalid")
        workflow_run = artifact.get("workflow_run")
        if isinstance(workflow_run, dict) and str(workflow_run.get("id")) != run_id:
            raise RecoveryArtifactError("release bundle belongs to a different workflow run")
        return {
            "prior_failure_run_id": run_id,
            "artifact_run_id": run_id,
            "artifact_id": artifact_id,
            "artifact_name": bundle_name,
            "artifact_digest": digest,
        }
    return {
        "prior_failure_run_id": str(candidates[0]["id"]),
        "artifact_run_id": "",
        "artifact_id": "",
        "artifact_name": bundle_name,
        "artifact_digest": "",
    }


def api_json(api_root: str, token: str, path: str) -> Any:
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}{path}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "dockrev-release-recovery",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.loads(response.read().decode(response.headers.get_content_charset() or "utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        raise RecoveryArtifactError(f"GitHub API request failed: {error.code}: {detail[:400]}") from error
    except urllib.error.URLError as error:
        raise RecoveryArtifactError(f"GitHub API request failed: {error.reason}") from error


def resolve_recovery_artifact(
    api_root: str, token: str, repository: str, merge_sha: str,
) -> dict[str, Any]:
    owner, separator, name = repository.partition("/")
    if not separator or not owner or not name:
        raise RecoveryArtifactError("repository must be owner/name")
    if not re.fullmatch(r"[0-9a-f]{40}", merge_sha):
        raise RecoveryArtifactError("merge SHA must be a 40-character lowercase SHA")
    def list_artifacts(artifact_name: str) -> list[dict[str, Any]]:
        items: list[dict[str, Any]] = []
        page = 1
        while True:
            query = urllib.parse.urlencode({"name": artifact_name, "per_page": 100, "page": page})
            result = api_json(api_root, token, f"/repos/{owner}/{name}/actions/artifacts?{query}")
            page_items = result.get("artifacts") if isinstance(result, dict) else None
            if not isinstance(page_items, list):
                raise RecoveryArtifactError("GitHub did not return release artifact records")
            items.extend(page_items)
            if len(page_items) < 100:
                return items
            page += 1

    identity_name = f"release-intent-{merge_sha}"
    bundle_name = f"release-bundle-{merge_sha}"
    identity_artifacts = list_artifacts(identity_name)
    bundle_artifacts = list_artifacts(bundle_name)
    run_ids = {
        str((item.get("workflow_run") or {}).get("id", ""))
        for item in identity_artifacts
        if item.get("name") == identity_name
    }
    runs = []
    for run_id in sorted(run_ids):
        if not re.fullmatch(r"[1-9][0-9]*", run_id):
            continue
        run = api_json(api_root, token, f"/repos/{owner}/{name}/actions/runs/{run_id}")
        if not isinstance(run, dict):
            raise RecoveryArtifactError("prior Release run metadata is invalid")
        runs.append(run)
    return select_reusable_artifact(runs, identity_artifacts, bundle_artifacts, merge_sha)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--merge-sha", required=True)
    parser.add_argument("--token", default="")
    parser.add_argument("--api-root", default="https://api.github.com")
    args = parser.parse_args()
    if not args.token:
        parser.error("--token is required")
    try:
        print(json.dumps(resolve_recovery_artifact(args.api_root, args.token, args.repository, args.merge_sha), sort_keys=True))
    except RecoveryArtifactError as error:
        print(str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
