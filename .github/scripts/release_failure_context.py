#!/usr/bin/env python3
"""Validate and render the structured context consumed by the release notifier."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any

import release_policy


def notification_summary(payload: dict[str, Any]) -> str:
    release_policy.validate_failure_context(payload)
    lines = [
        f"Dockrev release failed - {payload['tag']}",
        "status: failure",
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
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()
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
