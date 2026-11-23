#!/usr/bin/env python3
"""Static contracts for the target-only manual release surface."""

from __future__ import annotations

import importlib.util
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


policy = json.loads(read(".github/manual-version-release.json"))
assert policy["schema_version"] == 2
assert policy["baseline"] == {"source": "github-final-release", "base_branch": "main"}
assert policy["source_ci_workflow"] == "CI (main)"
assert "identity" not in policy and "reservation" not in json.dumps(policy)
assert policy["publication"]["prerelease"]["ghcr_latest"] is False
assert policy["publication"]["prerelease"]["ghcr_source_sha"] is True

release = read(".github/workflows/release.yml")
assert "workflow_dispatch:" in release
inputs = release.split("  workflow_dispatch:", 1)[1].split("\npermissions:", 1)[0]
assert re.findall(r"^\s{6}([a-z_]+):", inputs, re.MULTILINE) == ["target"]
assert "push:" not in release.split("permissions:", 1)[0]
assert "merge_sha" not in release and "recovery_reason" not in release
assert "release_intent.py intent" in release
assert "release_intent.py baseline" in release
assert "release_intent.py range" in release
assert "release_intent.py resolve" in release
assert "ci-main.yml" in release
assert "sha-${SOURCE_SHA}" in release
assert "release-images.json" in release
assert "release-bundle-${{ needs.preflight.outputs.source_sha }}" in release
assert "existing" in release and "refusing to overwrite" in release
assert "release-plan" not in release
for obsolete in ("release-reservation/v", "release-publication-lock/v", "identity_json", "artifact_run_id"):
    assert obsolete not in release, obsolete

for path in (
    ".github/workflows/release-preparation.yml",
    ".github/workflows/release-completion-pr.yml",
    ".github/workflows/release-abort.yml",
    ".github/scripts/release_preparation.py",
    ".github/scripts/release_completion.py",
    ".github/scripts/release_identity.py",
    ".github/scripts/release_abort.py",
    ".github/scripts/release_recovery_artifact.py",
    ".github/scripts/release_baseline.py",
    "VERSION",
):
    assert not (ROOT / path).exists(), path

ci_main = read(".github/workflows/ci-main.yml")
assert "name: CI (main)" in ci_main
assert "branches: [main]" in ci_main
assert "release-channel-contract-check.sh" in ci_main

quality = json.loads(read(".github/quality-gates.json"))
assert quality["required_checks"] == ["Review Policy Gate", "Quality Gates Contract"]
assert "retiring_required_checks" not in quality
assert "pending_required_checks" not in quality
assert quality["policy"]["branch_protection"]["strict_required_status_checks"] is True
assert quality["policy"]["branch_protection"]["require_extra_approval_for_unattributed_changes"] is True
assert "required_check_migration" not in quality
workflow_paths = {
    "Quality Gates Contract": ".github/workflows/quality-gates-contract.yml",
    "Review Policy": ".github/workflows/review-policy.yml",
    "CI (PR)": ".github/workflows/ci-pr.yml",
}
for expected in quality["expected_pr_workflows"]:
    workflow_text = read(workflow_paths[expected["workflow"]])
    assert f"name: {expected['workflow']}" in workflow_text
    for job_name in expected["jobs"]:
        assert f"name: {job_name}" in workflow_text

spec = importlib.util.spec_from_file_location(
    "check_live_quality_gates", ROOT / ".github/scripts/check-live-quality-gates.py"
)
assert spec and spec.loader
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)
assert checker.required_check_state(quality, ["Quality Gates Contract", "Review Policy Gate"]) == "target"
retiring_declaration = {**quality, "retiring_required_checks": ["Manual Version Release Completion"]}
assert checker.required_check_state(
    retiring_declaration, ["Manual Version Release Completion", "Quality Gates Contract", "Review Policy Gate"]
) == "retiring"
assert checker.required_check_state(quality, ["Review Policy Gate"]) == "drift"
assert checker.required_check_state(
    {**quality, "pending_required_checks": ["Quality Gates Contract"]},
    ["Review Policy Gate"],
) == "pending"

quality_contract = read(".github/workflows/quality-gates-contract.yml")
assert "pull_request_target:" in quality_contract
assert "ref: ${{ github.workflow_sha }}" in quality_contract
assert "queue: max" in quality_contract

review_policy = read(".github/workflows/review-policy.yml")
assert "queue: max" in review_policy and "cancel-in-progress: true" not in review_policy

notification = json.loads(read(".github/release-failure-notification.json"))
assert "Label Gate" not in notification["expected_success_workflows"]
assert {"Release", "CI (main)", "Quality Gates Contract"} <= set(notification["expected_success_workflows"])
assert "Release Preparation" not in notification["expected_success_workflows"]
assert "Release completion" not in notification["expected_success_workflows"]
generic_notifier = read(".github/workflows/notify-failed-workflow.yml")
assert "CI (main)" in generic_notifier
assert "Release Preparation" not in generic_notifier

preparation_creates_pr = re.search(r"POST[^\n]*/pulls", read(".github/scripts/release_intent.py"))
assert preparation_creates_pr is None

print("PASS: target-only manual release workflow and publication contracts")
