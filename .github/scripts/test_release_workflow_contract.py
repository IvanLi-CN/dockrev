#!/usr/bin/env python3
"""Static contract checks for the trusted release workflow topology."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


label_gate = text(".github/workflows/label-gate.yml")
completion = text(".github/workflows/release-completion.yml")
preparation = text(".github/workflows/release-preparation.yml")
release = text(".github/workflows/release.yml")
notify = text(".github/workflows/notify-release-failure.yml")
ci_pr = text(".github/workflows/ci-pr.yml")

assert "name: Label Gate" in label_gate and "pull_request_target:" in label_gate
assert "name: Release completion" in completion and "pull_request_target:" in completion
assert "workflows: [\"CI (PR)\"]" in preparation
assert "createCommitOnBranch" in text(".github/scripts/release_preparation.py")
assert "branches: [main]" in release and "merge_sha:" in release and "recovery_reason:" in release
assert "release-failure-context-" in release and "workflow_dispatch merge_sha=" in release
assert "oidrune/.github/workflows/notify.yml@" in notify
assert "required_secrets" in text(".github/release-failure-notification.json")
assert "paths-ignore:" in ci_pr and "- VERSION" in ci_pr
assert "Release Candidate Pipeline" not in release
assert "release_readiness.py" not in release
assert "refs/notes/release" not in release

quality = json.loads((ROOT / ".github/quality-gates.json").read_text(encoding="utf-8"))
assert quality["required_checks"] == ["Review Policy Gate", "Label Gate", "Release completion"]
assert quality["policy"]["branch_protection"]["require_merge_queue"] is False

print("PASS: release workflow contract")
