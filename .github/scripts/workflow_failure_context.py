#!/usr/bin/env python3
"""Render a safe notification summary for an unexpected workflow failure."""

from __future__ import annotations

import os
import re
import uuid
from collections.abc import Mapping
from pathlib import Path


def _clean(value: str | None, fallback: str = "unknown") -> str:
    cleaned = " ".join((value or "").split())
    return cleaned or fallback


def _number(value: str | None, field: str) -> str:
    cleaned = _clean(value, "")
    if not re.fullmatch(r"[0-9]+", cleaned):
        raise ValueError(f"{field} must be a decimal number")
    return cleaned


def render_summary(context: Mapping[str, str]) -> str:
    conclusion = _clean(context.get("conclusion"))
    if conclusion != "failure":
        raise ValueError("workflow failure context must describe a failed run")

    run_id = _number(context.get("run_id"), "run_id")
    attempt = _number(context.get("run_attempt"), "run_attempt")
    repository = _clean(context.get("repository"))
    server = _clean(context.get("server"), "https://github.com").rstrip("/")
    run_url = f"{server}/{repository}/actions/runs/{run_id}"
    head_sha = _clean(context.get("head_sha"))
    if not re.fullmatch(r"[0-9a-fA-F]{40}", head_sha):
        head_sha = "unknown"

    lines = [
        "Dockrev workflow failed",
        "status: failure",
        f"repository: {repository}",
        f"workflow: {_clean(context.get('workflow'))}",
        f"event: {_clean(context.get('event'))}",
        f"ref: {_clean(context.get('ref'))}",
        f"branch: {_clean(context.get('branch'))}",
        f"head sha: {head_sha}",
        f"pull request: {_clean(context.get('pull_request'), 'none')}",
        f"attempt: {attempt}",
        f"actor: {_clean(context.get('actor'))}",
        f"run: {run_url}",
        "recovery: inspect failed jobs and rerun this workflow only after fixing the root cause",
    ]
    return "\n".join(lines)


def _context_from_environment() -> dict[str, str]:
    return {
        "repository": os.environ.get("GITHUB_REPOSITORY", ""),
        "server": os.environ.get("GITHUB_SERVER_URL", ""),
        "workflow": os.environ.get("WORKFLOW_NAME", ""),
        "event": os.environ.get("WORKFLOW_EVENT", ""),
        "ref": os.environ.get("WORKFLOW_REF", ""),
        "branch": os.environ.get("WORKFLOW_BRANCH", ""),
        "head_sha": os.environ.get("WORKFLOW_HEAD_SHA", ""),
        "pull_request": os.environ.get("WORKFLOW_PULL_REQUEST", ""),
        "run_id": os.environ.get("WORKFLOW_RUN_ID", ""),
        "run_attempt": os.environ.get("WORKFLOW_RUN_ATTEMPT", ""),
        "actor": os.environ.get("WORKFLOW_ACTOR", ""),
        "conclusion": os.environ.get("WORKFLOW_CONCLUSION", ""),
    }


def main() -> int:
    summary = render_summary(_context_from_environment())
    output_path = os.environ.get("GITHUB_OUTPUT")
    if not output_path:
        print(summary)
        return 0

    delimiter = f"codex_summary_{uuid.uuid4().hex}"
    with Path(output_path).open("a", encoding="utf-8") as output:
        output.write(f"summary<<{delimiter}\n{summary}\n{delimiter}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
