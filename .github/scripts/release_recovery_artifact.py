#!/usr/bin/env python3
"""Select an immutable artifact bundle for same-identity release recovery."""

from __future__ import annotations

import argparse
import io
import json
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from typing import Any


class RecoveryArtifactError(ValueError):
    pass


class RecoveryBundleUnavailable(RecoveryArtifactError):
    pass


RELEASE_WORKFLOW_PATH = ".github/workflows/release.yml"


def is_release_workflow_path(path: Any) -> bool:
    if not isinstance(path, str):
        return False
    if path == RELEASE_WORKFLOW_PATH:
        return True
    prefix = f"{RELEASE_WORKFLOW_PATH}@"
    return path.startswith(prefix) and bool(path[len(prefix):])


def recovery_candidates(
    runs: list[dict[str, Any]], identity_artifacts: list[dict[str, Any]], merge_sha: str,
) -> list[dict[str, Any]]:
    run_by_id = {str(run.get("id", "")): run for run in runs}
    identity_name = f"release-intent-{merge_sha}"
    identity_run_ids = {
        str((item.get("workflow_run") or {}).get("id", ""))
        for item in identity_artifacts
        if item.get("name") == identity_name and item.get("expired") is not True
    }
    candidates = []
    for run_id in identity_run_ids:
        run = run_by_id.get(run_id)
        if (
            not run
            or not is_release_workflow_path(run.get("path"))
            or run.get("conclusion") != "failure"
            or run.get("head_branch") != "main"
        ):
            continue
        same_identity_run = (
            run.get("event") == "push" and run.get("head_sha") == merge_sha
        ) or run.get("event") == "workflow_dispatch"
        if same_identity_run:
            candidates.append(run)
    candidates.sort(key=lambda item: str(item.get("updated_at", "")), reverse=True)
    if not candidates:
        raise RecoveryArtifactError("same-SHA recovery requires a prior failed Release run with this immutable identity")
    for run in candidates:
        if not re.fullmatch(r"[1-9][0-9]*", str(run.get("id", ""))):
            raise RecoveryArtifactError("prior Release run identity is invalid")
    return candidates


def canonical_digest(value: str, label: str) -> str:
    if not isinstance(value, str) or not re.fullmatch(r"(?:sha256:)?[0-9a-f]{64}", value):
        raise RecoveryArtifactError(f"{label} is missing or invalid")
    return value if value.startswith("sha256:") else f"sha256:{value}"


def failure_context_digest_from_zip(
    content: bytes, *, repository: str, merge_sha: str, run_id: str, attempt: int,
) -> str:
    try:
        with zipfile.ZipFile(io.BytesIO(content)) as archive:
            files = [info for info in archive.infolist() if not info.is_dir()]
            if len(files) != 1 or files[0].filename != "release-failure-context.json":
                raise RecoveryArtifactError("prior release failure context archive is invalid")
            if files[0].file_size > 1024 * 1024:
                raise RecoveryArtifactError("prior release failure context is too large")
            payload = json.loads(archive.read(files[0]).decode("utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, zipfile.BadZipFile) as error:
        raise RecoveryArtifactError("prior release failure context archive is invalid") from error
    run_suffix = f"/{repository}/actions/runs/{run_id}"
    if (
        not isinstance(payload, dict)
        or payload.get("repository") != repository
        or payload.get("merge_commit_sha") != merge_sha
        or payload.get("run_attempt") != attempt
        or not isinstance(payload.get("run_url"), str)
        or not payload["run_url"].rstrip("/").endswith(run_suffix)
        or not isinstance(payload.get("artifact_digest"), str)
    ):
        raise RecoveryArtifactError("prior release failure context does not match the recovery identity")
    digest = payload["artifact_digest"]
    return canonical_digest(digest, "failure-context artifact digest") if digest else ""


def select_reusable_artifact(
    runs: list[dict[str, Any]], identity_artifacts: list[dict[str, Any]],
    bundle_artifacts: list[dict[str, Any]], merge_sha: str,
    failure_context_digests: dict[str, str] | None = None,
) -> dict[str, Any]:
    candidates = recovery_candidates(runs, identity_artifacts, merge_sha)
    bundle_name = f"release-bundle-{merge_sha}"
    context_digests = failure_context_digests or {}
    recorded_digests = {
        canonical_digest(digest, "failure-context artifact digest")
        for digest in context_digests.values()
        if digest
    }
    expired_or_invalid_bundle = False
    unavailable_bundle_digests = set()
    for run in candidates:
        run_id = str(run.get("id", ""))
        matches = [
            item for item in bundle_artifacts
            if item.get("name") == bundle_name
            and str((item.get("workflow_run") or {}).get("id", "")) == run_id
        ]
        if len(matches) > 1:
            raise RecoveryArtifactError("prior Release run has duplicate immutable bundle artifacts")
        if not matches:
            continue
        artifact = matches[0]
        if artifact.get("expired") is not False:
            expired_or_invalid_bundle = True
            expired_digest = artifact.get("digest")
            unavailable_bundle_digests.add(
                canonical_digest(str(expired_digest or ""), "expired release bundle digest")
            )
            continue
        digest = str(artifact.get("digest", ""))
        digest = canonical_digest(digest, "prior release bundle digest")
        if recorded_digests and recorded_digests != {digest}:
            raise RecoveryArtifactError("prior Release failure context and bundle digests do not match")
        if unavailable_bundle_digests and unavailable_bundle_digests != {digest}:
            raise RecoveryArtifactError("prior expired bundle and reusable bundle digests do not match")
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

    if expired_or_invalid_bundle:
        raise RecoveryArtifactError(
            "a completed release bundle is expired or has invalid metadata; restore the original bundle before retrying"
        )
    candidate_ids = {str(run["id"]) for run in candidates}
    if failure_context_digests is None:
        raise RecoveryBundleUnavailable("prior failure context is required to determine whether a bundle was completed")
    if candidate_ids - set(failure_context_digests):
        raise RecoveryArtifactError(
            "prior release failure context is unavailable; inspect the original run before retrying"
        )
    if recorded_digests:
        raise RecoveryArtifactError(
            "prior failure context proves a bundle was completed but its original artifact is missing; restore that bundle before retrying"
        )
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
    opener = urllib.request.build_opener(ArtifactRedirectHandler)
    try:
        with opener.open(request, timeout=30) as response:
            return json.loads(response.read().decode(response.headers.get_content_charset() or "utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        raise RecoveryArtifactError(f"GitHub API request failed: {error.code}: {detail[:400]}") from error
    except urllib.error.URLError as error:
        raise RecoveryArtifactError(f"GitHub API request failed: {error.reason}") from error


class ArtifactRedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        redirected = super().redirect_request(req, fp, code, msg, headers, newurl)
        if redirected:
            source = urllib.parse.urlsplit(req.full_url)
            destination = urllib.parse.urlsplit(newurl)
            if source.scheme != destination.scheme or source.netloc != destination.netloc:
                redirected.remove_header("Authorization")
        return redirected


def api_bytes(api_root: str, token: str, path: str) -> bytes:
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}{path}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "dockrev-release-recovery",
        },
    )
    opener = urllib.request.build_opener(ArtifactRedirectHandler)
    try:
        with opener.open(request, timeout=30) as response:
            content = response.read(1024 * 1024 + 1)
            if len(content) > 1024 * 1024:
                raise RecoveryArtifactError("prior release failure context archive is too large")
            return content
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        raise RecoveryArtifactError(f"GitHub artifact download failed: {error.code}: {detail[:400]}") from error
    except urllib.error.URLError as error:
        raise RecoveryArtifactError(f"GitHub artifact download failed: {error.reason}") from error


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
    candidates = recovery_candidates(runs, identity_artifacts, merge_sha)
    contexts: dict[str, str] = {}
    for run in candidates:
        run_id = str(run["id"])
        attempt = run.get("run_attempt")
        if not isinstance(attempt, int) or attempt < 1:
            raise RecoveryArtifactError("prior Release run attempt is invalid")
        context_name = f"release-failure-context-{run_id}-{attempt}"
        context_artifacts = list_artifacts(context_name)
        if not context_artifacts:
            continue
        if len(context_artifacts) != 1:
            raise RecoveryArtifactError("prior Release run has duplicate failure context artifacts")
        artifact = context_artifacts[0]
        if artifact.get("name") != context_name:
            raise RecoveryArtifactError("prior Release failure context artifact name does not match the expected run")
        workflow_run = artifact.get("workflow_run")
        if not isinstance(workflow_run, dict) or str(workflow_run.get("id", "")) != run_id:
            raise RecoveryArtifactError("prior failure context belongs to a different workflow run")
        if artifact.get("expired") is not False:
            continue
        artifact_id = str(artifact.get("id", ""))
        if not re.fullmatch(r"[1-9][0-9]*", artifact_id):
            raise RecoveryArtifactError("prior Release failure context artifact ID is invalid")
        content = api_bytes(
            api_root,
            token,
            f"/repos/{owner}/{name}/actions/artifacts/{artifact_id}/zip",
        )
        contexts[run_id] = failure_context_digest_from_zip(
            content,
            repository=repository,
            merge_sha=merge_sha,
            run_id=run_id,
            attempt=attempt,
        )
    return select_reusable_artifact(
        runs,
        identity_artifacts,
        bundle_artifacts,
        merge_sha,
        failure_context_digests=contexts,
    )


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
