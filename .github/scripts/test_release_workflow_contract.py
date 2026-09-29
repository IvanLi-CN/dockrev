#!/usr/bin/env python3
"""Static contracts for single-input release preparation and publication."""

from __future__ import annotations

import importlib.util
import json
import re
from pathlib import Path

import release_policy


ROOT = Path(__file__).resolve().parents[2]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


policy_path = ROOT / ".github/manual-version-release.json"
assert policy_path.is_file()
policy = json.loads(policy_path.read_text(encoding="utf-8"))
assert policy["baseline"] == {"ref": "main", "file": "VERSION"}
assert policy["intents"] == ["major", "minor", "patch", "alpha", "beta", "rc"]
assert policy["prerelease_channels"] == ["alpha", "beta", "rc"]
assert policy["prerelease_sequence_start"] == 1
assert policy["publication"]["prerelease"]["ghcr_latest"] is False

preparation = read(".github/workflows/release-preparation.yml")
inputs = preparation.split("  workflow_dispatch:", 1)[1].split("\npermissions:", 1)[0]
declared_inputs = re.findall(r"^\s{6}([a-z_]+):", inputs, re.MULTILINE)
assert declared_inputs == ["version"], declared_inputs
assert "required: true" in inputs and "type: string" in inputs
for forbidden in ("pr_number", "release_mode", "covered_merge_sha", "components", "exact_version", "baseline_version"):
    assert not re.search(rf"^\s{{6}}{re.escape(forbidden)}:", inputs, re.MULTILINE), forbidden
assert "workflow_run:" not in preparation
assert "cancel-in-progress: false" in preparation
assert "pull-requests: write" in preparation and "contents: write" in preparation
assert "release_preparation.py" in preparation and "--version" in preparation

preparation_script = read(".github/scripts/release_preparation.py")
assert "/git/ref/heads/main" in preparation_script
assert "release_policy.compute_target(baseline, args.version)" in preparation_script
assert "current main commit" in preparation_script
assert "source VERSION does not match its frozen baseline" in preparation_script
assert "Release-Intent: {decision['version_input']}" in preparation_script
assert '"path": "VERSION"' in preparation_script
assert "release-reservation/v{version}" in preparation_script
assert "release-preparation/v{decision['version']}" in preparation_script
assert "find_or_create_pull_request" in preparation_script
assert "covered_merge_sha" not in preparation_script
assert "release_mode" not in preparation_script

for obsolete in (
    ".github/pr-label-release.json",
    ".github/workflows/label-gate.yml",
    ".github/scripts/label-gate.sh",
    ".agents/skills/pr-label-release/SKILL.md",
):
    assert not (ROOT / obsolete).exists(), obsolete

quality = json.loads(read(".github/quality-gates.json"))
assert quality["required_checks"] == ["Review Policy Gate", "Manual Version Release Completion"]
assert quality["required_check_migration"]["source_required_checks"] == [
    "Review Policy Gate", "Label Gate", "Release completion"
]
assert quality["required_check_migration"]["target_required_checks"] == quality["required_checks"]
assert "trusted target workflow is present on main" in quality["required_check_migration"]["cutover_condition"]
assert "must succeed for each PR before merge" in quality["required_check_migration"]["cutover_condition"]
target_workflow_contract = quality["required_check_migration"]["target_workflow"]
assert target_workflow_contract["branch"] == "main"
assert target_workflow_contract["path"] == ".github/workflows/release-completion-pr.yml"
assert target_workflow_contract["check_context"] in quality["required_check_migration"]["target_required_checks"]
assert "release_label_contract" not in quality
quality_gate_checker_spec = importlib.util.spec_from_file_location(
    "check_live_quality_gates", ROOT / ".github/scripts/check-live-quality-gates.py"
)
assert quality_gate_checker_spec and quality_gate_checker_spec.loader
quality_gate_checker = importlib.util.module_from_spec(quality_gate_checker_spec)
quality_gate_checker_spec.loader.exec_module(quality_gate_checker)
assert quality_gate_checker.required_check_migration_state(
    quality, ["Label Gate", "Release completion", "Review Policy Gate"]
) == "source"
assert quality_gate_checker.required_check_migration_state(
    quality, ["Manual Version Release Completion", "Review Policy Gate"]
) == "target"
assert quality_gate_checker.required_check_migration_state(
    quality, ["Review Policy Gate"]
) == "drift"
target_workflow = read(".github/workflows/release-completion-pr.yml")
assert quality_gate_checker.target_workflow_status(quality, target_workflow) == "trusted"
assert quality_gate_checker.target_workflow_status(quality, None) == "missing"
assert quality_gate_checker.target_workflow_status(quality, "pull_request:\n  branches: [main]") == "untrusted"
source_readiness = quality_gate_checker.migration_readiness_record(
    "source", "trusted", "Manual Version Release Completion",
    ["Label Gate", "Release completion", "Review Policy Gate"],
)
assert source_readiness["trusted_target_workflow_on_main"] is True
assert source_readiness["target_context_required"] is False
assert source_readiness["target_gate_active"] is False
target_readiness = quality_gate_checker.migration_readiness_record(
    "target", "trusted", "Manual Version Release Completion",
    ["Manual Version Release Completion", "Review Policy Gate"],
)
assert target_readiness["target_context_required"] is True
assert target_readiness["target_gate_active"] is True
workflow_paths = {
    "Review Policy": ".github/workflows/review-policy.yml",
    "Release completion": ".github/workflows/release-completion-pr.yml",
    "CI (PR)": ".github/workflows/ci-pr.yml",
}
for expected in quality["expected_pr_workflows"]:
    workflow_text = read(workflow_paths[expected["workflow"]])
    assert f"name: {expected['workflow']}" in workflow_text
    for job_name in expected["jobs"]:
        assert f"name: {job_name}" in workflow_text
assert all(item["workflow"] != "Release Preparation" for item in quality["expected_pr_workflows"])

completion_workflow = read(".github/workflows/release-completion-pr.yml")
assert "pull_request_target:" in completion_workflow and "\n  pull_request:" not in completion_workflow
assert "github.event.pull_request.number == 421" not in completion_workflow
assert "github.event.pull_request.head.sha" not in completion_workflow.split("Checkout trusted release helpers", 1)[1].split("- name: Verify", 1)[0]
assert "ref: ${{ github.workflow_sha }}" in completion_workflow
assert "name: Manual Version Release Completion" in completion_workflow
assert "labeled" not in completion_workflow and "unlabeled" not in completion_workflow
assert "--pull-number" in completion_workflow
completion_script = read(".github/scripts/release_completion.py")
assert "main:VERSION changed after this release identity was prepared" in completion_script
assert "immutable version reservation does not point to this identity" in completion_script
assert "publication lock belongs to another identity" in completion_script
assert "release_policy.compute_target" in completion_script
assert "no-release-identity" in completion_script
assert "VERSION changes require a signed release identity" in completion_script

ci_pr = read(".github/workflows/ci-pr.yml")
assert "Release-Mode" not in ci_pr and "Covered-Product-Merge-SHA" not in ci_pr
assert "type:(patch|minor|major)" not in ci_pr
assert "(alpha|beta|rc)" in ci_pr
assert "(alpha|beta|rc|dev)" not in ci_pr

release = read(".github/workflows/release.yml")
assert "branches: [main]" in release
assert "merge_sha:" in release and "recovery_reason:" in release
assert "cancel-in-progress: false" in release
assert "release-preparation/v${VERSION}" in release
assert "release-bundle-${{ needs.identity.outputs.merge_sha }}" in release
assert "release-bundle-${{ env.MERGE_SHA }}" in release
assert "ARTIFACT_DIGEST" in release and "artifact digest" in release.lower()
assert "ref: ${{ github.sha }}" in release
assert "check_release_artifact_digest.py" in release
assert "jq --arg name" not in release
publish = release.split("  publish:", 1)[1]
assert "ref: ${{ github.sha }}" in publish
assert "path: .workflow-src" in publish
assert "release_artifact_bundle.py verify" in release
assert "release_recovery_artifact.py" in release
assert "same-sha-recovery-preflight" in release
assert "restore its exact bundle and digest before retrying" in release
assert release_policy.SAME_SHA_RECOVERY_PRECHECK_INSTRUCTION in release
assert "artifact_run_id" in release and "run-id:" in release
assert "actions/read" not in release
assert "actions: read" in release
assert "prerelease: ${{ env.CHANNEL != 'stable' }}" in release
assert "if: env.CHANNEL == 'stable'" in release
assert 'source="release-assets/${arch}/musl"' not in release
assert "release-bundle/release-assets/${arch}/musl" in release
assert "release-bundle/dist/release/*.tar.gz" in release
assert "release_readiness.py" not in release and "release queue" not in release
assert "release-publication-lock/v${lock_version}" in release

bundle = read(".github/scripts/release_artifact_bundle.py")
assert "identity_projection" in bundle and "content_digest" in bundle
assert "release_tag" in bundle and "merge_commit_sha" in bundle
recovery = read(".github/scripts/release_recovery_artifact.py")
assert "release-intent-{merge_sha}" in recovery
assert "release-bundle-{merge_sha}" in recovery
assert "def is_release_workflow_path" in recovery
assert '"artifact_digest": digest' in recovery
assert "failure_context_digest_from_zip" in recovery
assert "prior failure context proves a bundle was completed" in recovery
assert "release-failure-context-{run_id}-{attempt}" in recovery
assert "actions/artifacts/{artifact_id}/zip" in recovery
assert "prior failure context belongs to a different workflow run" in recovery

notification = json.loads(read(".github/release-failure-notification.json"))
assert "Label Gate" not in notification["expected_success_workflows"]
assert {"Release", "Release Preparation", "Release completion"} <= set(notification["expected_success_workflows"])
generic_notifier = read(".github/workflows/notify-failed-workflow.yml")
assert "Label Gate" not in generic_notifier
assert "Release Preparation" in generic_notifier

print("PASS: manual version release workflow and publication contracts")
