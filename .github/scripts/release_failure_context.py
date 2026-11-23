#!/usr/bin/env python3
"""Validate and render the release failure context for notifications."""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any

import release_policy


def notification_summary(payload: dict[str, Any]) -> str:
    release_policy.validate_failure_context(payload)
    lines = [
        "Dockrev release failed",
        "status: failure",
        f"repository: {payload.get('repository', '')}",
        f"workflow: {payload.get('workflow', 'Release')}",
        f"event: {payload.get('event', '')}",
        f"ref: {payload.get('ref', '')}",
        f"attempt: {payload.get('run_attempt', '')}",
        f"actor: {payload.get('actor', '')}",
        f"target: {payload['target']}",
        f"source sha: {payload['source_sha']}",
    ]
    if payload.get("version"):
        lines.extend(
            [
                f"baseline VERSION: {payload['baseline_version']}",
                f"version: {payload['version']} ({payload['channel']})",
                f"tag: {payload['tag']}",
                f"artifact digest: {payload.get('artifact_digest') or 'not created before this failure'}",
                "assets: " + ", ".join(payload["artifact_names"]),
            ]
        )
        digests = release_policy.validate_image_digests(payload.get("image_digests"))
        if digests:
            lines.append(
                "image digests: " + ", ".join(f"{name}={digest}" for name, digest in sorted(digests.items()))
            )
    else:
        lines.append("identity: preflight did not resolve a release version")
    lines.extend([f"run: {payload['run_url']}", f"recovery: {payload['recovery_instruction']}"])
    return "\n".join(lines)


def unresolved_summary(
    *, repository: str, server: str, run_id: str, attempt: str, event: str, ref: str, actor: str,
) -> str:
    return "\n".join(
        [
            "Dockrev release failed",
            "status: failure",
            f"repository: {repository}",
            "workflow: Release",
            f"event: {event}",
            f"ref: {ref}",
            f"attempt: {attempt}",
            f"actor: {actor}",
            "target: unavailable (failure context was not published)",
            "source sha: unavailable",
            f"run: {server}/{repository}/actions/runs/{run_id}",
            "recovery: inspect the failed run, then re-run Release from main with the same target",
        ]
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path)
    parser.add_argument("--unresolved-run", action="store_true")
    parser.add_argument("--repository")
    parser.add_argument("--server")
    parser.add_argument("--run-id")
    parser.add_argument("--attempt")
    parser.add_argument("--event")
    parser.add_argument("--ref")
    parser.add_argument("--actor")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        if args.unresolved_run:
            required = {
                "repository": args.repository,
                "server": args.server,
                "run_id": args.run_id,
                "attempt": args.attempt,
                "event": args.event,
                "ref": args.ref,
                "actor": args.actor,
            }
            missing = [name for name, value in required.items() if value is None]
            if missing:
                parser.error(f"unresolved run context missing: {', '.join(missing)}")
            summary = unresolved_summary(**required)
        elif args.input:
            payload = json.loads(args.input.read_text(encoding="utf-8"))
            summary = notification_summary(payload)
        else:
            parser.error("--input or --unresolved-run is required")
    except (OSError, json.JSONDecodeError, release_policy.PolicyError) as error:
        print(str(error), file=sys.stderr)
        return 2
    if args.output:
        args.output.write_text(summary, encoding="utf-8")
    elif os.environ.get("GITHUB_OUTPUT"):
        with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as handle:
            handle.write("summary<<EOF\n")
            handle.write(summary)
            handle.write("\nEOF\n")
    else:
        print(summary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
