#!/usr/bin/env python3
"""Validate source-build attestations produced by the candidate pipeline."""

from __future__ import annotations

import argparse
import json
import re
import urllib.request
import zipfile
from io import BytesIO
from pathlib import Path
from typing import Any


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
        "publish": False,
        "verification_mode": False,
    }
    for key, expected in required.items():
        if payload.get(key) != expected:
            return False, f"push attestation {key}={payload.get(key)!r} expected {expected!r}"
    if payload.get("scope") not in {"changed", "full"}:
        return False, "push attestation scope is invalid"
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


def download_attestation(api_root: str, repository: str, run_id: int, token: str, artifact_name: str) -> dict[str, Any]:
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}/repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100",
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        payload = json.load(response)
    for artifact in payload.get("artifacts", []):
        if artifact.get("name") != artifact_name or artifact.get("expired"):
            continue
        archive = api_bytes(artifact["archive_download_url"], token)
        with zipfile.ZipFile(BytesIO(archive)) as bundle:
            for member in bundle.namelist():
                if member.endswith(".json"):
                    return json.loads(bundle.read(member))
    raise ValueError(f"artifact {artifact_name} not found in Actions run {run_id}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("validate",))
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--target-sha", required=True)
    parser.add_argument("--kind", choices=("push", "verification", "full"), default="push")
    args = parser.parse_args()
    payload = json.loads(args.fixture.read_text())
    validator = {
        "push": validate_push_attestation,
        "full": validate_attestation,
        "verification": validate_verification_metrics,
    }[args.kind]
    valid, reason = validator(payload, args.target_sha)
    print(reason)
    return 0 if valid else 1


if __name__ == "__main__":
    raise SystemExit(main())
