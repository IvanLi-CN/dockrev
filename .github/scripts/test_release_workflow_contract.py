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
assert "workflows: [\"CI (PR)\", \"Label Gate\"]" in preparation
assert "group: release-preparation" in preparation
assert "actions: read" in preparation
assert "checks.create" not in preparation
assert "head_sha: process.env.HEAD_SHA" not in preparation
assert "checks: write" not in preparation
assert "actions: read" in completion
assert "createCommitOnBranch" in text(".github/scripts/release_preparation.py")
assert "branches: [main]" in release and "merge_sha:" in release and "recovery_reason:" in release
assert "release-failure-context-" in release and "workflow_dispatch merge_sha=" in release
assert "create VERSION-only release PR Covered-Product-Merge-SHA=" in release
assert "prior failed automatic Release run" in release
assert "path: release-assets" in release and 'chmod +x "${source}"' in release
assert "needs.identity.result == 'failure'" in release
assert "release-identity-failure-context-" in release
assert "release-publish-${{ needs.identity.outputs.merge_sha }}" in release
assert "tr '[:upper:]' '[:lower:]'" in release
assert "overwrite: true" in release
assert "github.run_attempt" in release
assert "oidrune/.github/workflows/notify.yml@" in notify
assert "name: release-failure-context-${{ github.event.workflow_run.id }}-${{ github.event.workflow_run.run_attempt }}" in notify
assert "EXPECTED_RELEASE_RUN_ID" in notify
assert "required_secrets" in text(".github/release-failure-notification.json")
assert "release-identity-guard:" in ci_pr
assert "Release-Mode" in ci_pr
assert "commit.data.commit.verification" in ci_pr
assert "commit.data.parents" in ci_pr
assert "files[0] === 'VERSION'" in ci_pr
assert "needs: [release-identity-guard]" in ci_pr[ci_pr.index("  unit-tests:"):]
assert "needs.release-identity-guard.outputs.skip != 'true'" in ci_pr
assert "release-identity-guard" in ci_pr
assert "Release Candidate Pipeline" not in release
assert "release_readiness.py" not in release
assert "refs/notes/release" not in release
assert "pull_requests" in text(".github/scripts/release_preparation.py")
assert "covered_product_has_identity" in text(".github/scripts/release_identity.py")
assert "pull_request_changed_files" in text(".github/scripts/release_completion.py")
assert "tag_is_reserved_by_other_pr" in text(".github/scripts/release_completion.py")

quality = json.loads((ROOT / ".github/quality-gates.json").read_text(encoding="utf-8"))
assert quality["required_checks"] == ["Review Policy Gate", "Label Gate", "Release completion"]
assert quality["policy"]["branch_protection"]["require_merge_queue"] is False

print("PASS: release workflow contract")
