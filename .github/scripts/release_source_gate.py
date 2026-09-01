#!/usr/bin/env python3
"""Find and validate the source-build gate for an exact release SHA."""

from __future__ import annotations

import argparse
import json
import os
import re
import time
import urllib.parse
import urllib.request
import zipfile
from io import BytesIO
from pathlib import Path
from typing import Any


WORKFLOW_FILE = "source-build-release-gate.yml"
FAST_WORKFLOW_FILE = "ci-main.yml"
VERIFICATION_WORKFLOW_FILE = "ci-gate-verification.yml"


def api_json(url: str, token: str) -> Any:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def api_bytes(url: str, token: str) -> bytes:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read()


def find_push_run(runs: list[dict[str, Any]], target_sha: str) -> dict[str, Any] | None:
    candidates = [
        run
        for run in runs
        if run.get("event") == "push"
        and run.get("head_branch") == "main"
        and run.get("head_sha") == target_sha
        and run.get("status") == "completed"
        and run.get("conclusion") == "success"
    ]
    return max(candidates, key=lambda run: run.get("id", 0), default=None)


def find_failed_push_run(runs: list[dict[str, Any]], target_sha: str) -> dict[str, Any] | None:
    candidates = [
        run
        for run in runs
        if run.get("event") == "push"
        and run.get("head_branch") == "main"
        and run.get("head_sha") == target_sha
        and run.get("status") == "completed"
        and run.get("conclusion") != "success"
    ]
    return max(candidates, key=lambda run: run.get("id", 0), default=None)


def validate_attestation(payload: dict[str, Any], target_sha: str) -> tuple[bool, str]:
    required = {
        "target_sha": target_sha,
        "gate": "source-build-release",
        "scope": "full",
        "docker": True,
        "web": True,
        "source_result": "success",
        "publish": False,
    }
    for key, expected in required.items():
        if payload.get(key) != expected:
            return False, f"attestation {key}={payload.get(key)!r} expected {expected!r}"
    return True, "ok"


def validate_push_attestation(payload: dict[str, Any], target_sha: str) -> tuple[bool, str]:
    required = {
        "target_sha": target_sha,
        "gate": "source-build-release",
        "scope": "changed",
        "publish": False,
        "verification_mode": False,
    }
    for key, expected in required.items():
        if payload.get(key) != expected:
            return False, f"push attestation {key}={payload.get(key)!r} expected {expected!r}"
    if not isinstance(payload.get("web"), bool) or not isinstance(payload.get("docker"), bool):
        return False, "push attestation web/docker markers must be booleans"
    source_result = payload.get("source_result")
    if payload["docker"] and source_result != "success":
        return False, f"Docker-scoped push must have source_result='success', got {source_result!r}"
    if not payload["docker"] and source_result not in ("skipped", "success"):
        return False, f"non-Docker push has invalid source_result={source_result!r}"
    return True, "ok"


def validate_verification_metrics(payload: dict[str, Any], target_sha: str) -> tuple[bool, str]:
    required = {
        "target_sha": target_sha,
        "fast_target_sha": target_sha,
        "scope": "full",
        "web": True,
        "docker": True,
        "fast_result": "success",
        "source_result": "success",
        "coverage_result": "success",
        "verification_mode": True,
        "publish": False,
    }
    for key, expected in required.items():
        if payload.get(key) != expected:
            return False, f"verification {key}={payload.get(key)!r} expected {expected!r}"
    if payload.get("storybook_shard_total") not in (2, 3):
        return False, "verification Storybook shard total is missing or invalid"
    if not isinstance(payload.get("storybook_story_count"), int) or payload["storybook_story_count"] <= 0:
        return False, "verification Storybook story count is missing or invalid"
    if not re.fullmatch(r"[0-9a-f]{64}", str(payload.get("coverage_summary", ""))):
        return False, "verification Storybook coverage summary is missing or invalid"
    return True, "ok"


def download_attestation(api_root: str, repository: str, run_id: int, token: str) -> dict[str, Any] | None:
    artifacts = api_json(f"{api_root}/repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100", token)
    for artifact in artifacts.get("artifacts", []):
        if artifact.get("name") != f"source-gate-attestation-{run_id}":
            continue
        archive = api_bytes(artifact["archive_download_url"], token)
        with zipfile.ZipFile(BytesIO(archive)) as bundle:
            for member in bundle.namelist():
                if member.endswith(".json"):
                    return json.loads(bundle.read(member))
    return None


def download_verification_metrics(api_root: str, repository: str, run_id: int, token: str) -> dict[str, Any] | None:
    artifacts = api_json(f"{api_root}/repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100", token)
    artifact_name = f"ci-gate-metrics-{run_id}"
    for artifact in artifacts.get("artifacts", []):
        if artifact.get("name") != artifact_name:
            continue
        archive = api_bytes(artifact["archive_download_url"], token)
        with zipfile.ZipFile(BytesIO(archive)) as bundle:
            for member in bundle.namelist():
                if member.endswith(".json"):
                    return json.loads(bundle.read(member))
    return None


def find_trusted_verification(
    *, api_root: str, repository: str, target_sha: str, token: str
) -> dict[str, Any] | None:
    query = urllib.parse.urlencode({"event": "workflow_dispatch", "branch": "main", "per_page": "100"})
    payload = api_json(
        f"{api_root}/repos/{repository}/actions/workflows/{VERIFICATION_WORKFLOW_FILE}/runs?{query}",
        token,
    )
    candidates = sorted(payload.get("workflow_runs", []), key=lambda run: run.get("id", 0), reverse=True)
    for run in candidates:
        if run.get("head_branch") != "main" or run.get("status") != "completed" or run.get("conclusion") != "success":
            continue
        metrics = download_verification_metrics(api_root, repository, int(run["id"]), token)
        if metrics is None:
            continue
        valid, _ = validate_verification_metrics(metrics, target_sha)
        if valid:
            return {"run_id": run["id"], "target_sha": target_sha, "source_gate": "verification"}
    return None


def wait_for_push_gate(
    *,
    api_root: str,
    repository: str,
    target_sha: str,
    token: str,
    timeout_seconds: int,
    poll_seconds: int,
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout_seconds
    query = urllib.parse.urlencode(
        {"event": "push", "branch": "main", "head_sha": target_sha, "per_page": "100"}
    )
    while True:
        source_payload = api_json(
            f"{api_root}/repos/{repository}/actions/workflows/{WORKFLOW_FILE}/runs?{query}",
            token,
        )
        fast_payload = api_json(
            f"{api_root}/repos/{repository}/actions/workflows/{FAST_WORKFLOW_FILE}/runs?{query}",
            token,
        )
        source_run = find_push_run(source_payload.get("workflow_runs", []), target_sha)
        fast_run = find_push_run(fast_payload.get("workflow_runs", []), target_sha)
        source_failed = find_failed_push_run(source_payload.get("workflow_runs", []), target_sha)
        fast_failed = find_failed_push_run(fast_payload.get("workflow_runs", []), target_sha)
        if source_failed or fast_failed:
            failed = source_failed or fast_failed
            raise RuntimeError(
                f"CI gate failed for target SHA {target_sha}: run {failed.get('id')} conclusion={failed.get('conclusion')}"
            )
        if source_run and fast_run:
            attestation = download_attestation(api_root, repository, int(source_run["id"]), token)
            if attestation is None:
                raise RuntimeError(f"source gate run {source_run['id']} has no exact-SHA attestation")
            valid, reason = validate_push_attestation(attestation, target_sha)
            if not valid:
                raise RuntimeError(f"source gate run {source_run['id']} attestation rejected: {reason}")
            return {
                "run_id": source_run["id"],
                "fast_run_id": fast_run["id"],
                "target_sha": target_sha,
                "source_gate": "push",
            }
        verification = find_trusted_verification(
            api_root=api_root, repository=repository, target_sha=target_sha, token=token
        )
        if verification:
            return verification
        if time.monotonic() >= deadline:
            raise RuntimeError(f"no successful fast and source gates for target SHA {target_sha} within {timeout_seconds}s")
        time.sleep(poll_seconds)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("wait", "validate"))
    parser.add_argument("--repository")
    parser.add_argument("--target-sha", required=True)
    parser.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))
    parser.add_argument("--api-root", default="https://api.github.com")
    parser.add_argument("--timeout-seconds", type=int, default=720)
    parser.add_argument("--poll-seconds", type=int, default=15)
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--kind", choices=("source", "verification"), default="source")
    args = parser.parse_args()

    if args.command == "validate":
        if not args.fixture:
            parser.error("validate requires --fixture")
        payload = json.loads(args.fixture.read_text())
        validator = validate_verification_metrics if args.kind == "verification" else validate_attestation
        valid, reason = validator(payload, args.target_sha)
        print(reason)
        return 0 if valid else 1

    if not args.repository or not args.token:
        parser.error("wait requires --repository and --token")
    result = wait_for_push_gate(
        api_root=args.api_root,
        repository=args.repository,
        target_sha=args.target_sha,
        token=args.token,
        timeout_seconds=args.timeout_seconds,
        poll_seconds=args.poll_seconds,
    )
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
