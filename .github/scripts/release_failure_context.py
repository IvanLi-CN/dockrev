#!/usr/bin/env python3
"""Validate and render the structured context consumed by the release notifier."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any

import release_policy


def resolved_identity_failure_context(
    identity: dict[str, Any], *, repository: str, server: str, run_id: str,
    attempt: str, event: str, ref: str, actor: str
) -> dict[str, Any]:
    release_policy.validate_identity(identity)
    version = str(identity["version"])
    expected = [
        suffix
        for base in [
            f"{binary}_{version}_linux_{arch}_{libc}"
            for arch in ("amd64", "arm64")
            for libc in ("gnu", "musl")
            for binary in ("dockrev", "dockrev-supervisor")
        ]
        for suffix in (f"{base}.tar.gz", f"{base}.tar.gz.sha256")
    ]
    merge_sha = str(identity["merge_commit_sha"])
    return {
        "pull_request": int(identity.get("pull_request") or 0),
        "source_sha": str(identity["source_sha"]),
        "merge_commit_sha": merge_sha,
        "type": str(identity["type"]),
        "channel": str(identity["channel"]),
        "version": version,
        "tag": str(identity["release_tag"]),
        "artifact_names": expected,
        "run_url": f"{server}/{repository}/actions/runs/{run_id}",
        "recovery_instruction": f"workflow_dispatch merge_sha={merge_sha} recovery_reason=<required>",
        "identity_resolution_failed": False,
        "identity_failure_kind": "identity-step-failure",
        "repository": repository,
        "workflow": "Release",
        "event": event,
        "ref": ref,
        "run_attempt": int(attempt),
        "actor": actor,
    }


def notification_summary(payload: dict[str, Any]) -> str:
    release_policy.validate_failure_context(
        payload,
        expected_repository=os.environ.get("GITHUB_REPOSITORY"),
        expected_run_id=os.environ.get("EXPECTED_RELEASE_RUN_ID"),
        expected_server=os.environ.get("GITHUB_SERVER_URL"),
        expected_attempt=os.environ.get("EXPECTED_RELEASE_ATTEMPT"),
    )
    lines = [
        f"Dockrev release failed - {payload['tag']}",
        "status: failure",
        f"repository: {payload.get('repository', '')}",
        f"workflow: {payload.get('workflow', 'Release')}",
        f"event: {payload.get('event', '')}",
        f"ref: {payload.get('ref', '')}",
        f"attempt: {payload.get('run_attempt', '')}",
        f"actor: {payload.get('actor', '')}",
        f"intent: type:{payload['type']} channel:{payload['channel']}",
        f"version: {payload['version']}",
        f"source sha: {payload['source_sha']}",
        f"merge sha: {payload['merge_commit_sha']}",
        f"tag: {payload['tag']}",
        "assets: " + ", ".join(payload["artifact_names"]),
        f"run: {payload['run_url']}",
        f"recovery: {payload['recovery_instruction']}",
    ]
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path)
    parser.add_argument("--resolved-identity", type=Path)
    parser.add_argument("--repository")
    parser.add_argument("--server")
    parser.add_argument("--run-id")
    parser.add_argument("--attempt")
    parser.add_argument("--event")
    parser.add_argument("--ref")
    parser.add_argument("--actor")
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()
    if args.resolved_identity:
        required = {
            "repository": args.repository,
            "server": args.server,
            "run_id": args.run_id,
            "attempt": args.attempt,
            "event": args.event,
            "ref": args.ref,
            "actor": args.actor,
            "output": args.output,
        }
        missing = [name for name, value in required.items() if not value]
        if missing:
            parser.error(f"resolved identity context missing: {', '.join(missing)}")
        identity = json.loads(args.resolved_identity.read_text(encoding="utf-8"))
        payload = resolved_identity_failure_context(
            identity,
            repository=args.repository,
            server=args.server,
            run_id=args.run_id,
            attempt=args.attempt,
            event=args.event,
            ref=args.ref,
            actor=args.actor,
        )
        args.output.write_text(json.dumps(payload, sort_keys=True) + "\n", encoding="utf-8")
        return 0
    if not args.input:
        parser.error("--input or --resolved-identity is required")
    payload = json.loads(args.input.read_text(encoding="utf-8"))
    summary = notification_summary(payload)
    output = args.output or (Path(os.environ["GITHUB_OUTPUT"]) if os.environ.get("GITHUB_OUTPUT") else None)
    if output:
        with output.open("a", encoding="utf-8") as handle:
            handle.write("summary<<EOF\n")
            handle.write(summary)
            handle.write("\nEOF\n")
    else:
        print(summary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
