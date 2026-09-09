#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

echo "[contract-check] syntax and workflow YAML"
bash -n \
  .github/scripts/label-gate.sh \
  .github/scripts/release-intent.sh \
  .github/scripts/compute-version.sh \
  .github/scripts/test-release-snapshot.sh \
  .github/scripts/deploy-smoke.sh \
  .github/scripts/storybook-ci-check.sh
python3 -m py_compile \
  .github/scripts/release_snapshot.py \
  .github/scripts/release_preparation.py \
  .github/scripts/release_source_gate.py \
  .github/scripts/release_readiness.py \
  .github/scripts/test_release_preparation.py \
  .github/scripts/test_release_readiness.py \
  .github/scripts/test_ci_release_gate.py \
  .github/scripts/check-live-quality-gates.py \
  .github/scripts/resolve-ci-scope.py \
  .github/scripts/resolve-storybook-matrix.py
ruby -e 'require "yaml"; ARGV.each { |path| YAML.load_file(path) }' \
  .github/workflows/label-gate.yml \
  .github/workflows/review-policy.yml \
  .github/workflows/ci-pr.yml \
  .github/workflows/ci-main.yml \
  .github/workflows/source-build-release-gate.yml \
  .github/workflows/release-preparation.yml \
  .github/workflows/release-candidate.yml \
  .github/workflows/ci-gate-verification.yml \
  .github/workflows/release.yml \
  .github/workflows/notify-release-failure.yml

has_rg() { command -v rg >/dev/null 2>&1; }
search_fixed() {
  if has_rg; then rg -q -F -- "$1" "$2"; else grep -Fq -- "$1" "$2"; fi
}
ensure_absent() {
  if search_fixed "$1" "$2"; then
    echo "[contract-check] unexpected '$1' in $2" >&2
    exit 1
  fi
}

echo "[contract-check] release candidate and receipt invariants"
search_fixed 'name: Release Candidate Pipeline' .github/workflows/release-candidate.yml
search_fixed 'branches: [main]' .github/workflows/release-candidate.yml
search_fixed 'uses: ./.github/workflows/ci-main.yml' .github/workflows/release-candidate.yml
search_fixed 'uses: ./.github/workflows/source-build-release-gate.yml' .github/workflows/release-candidate.yml
search_fixed 'uses: ./.github/workflows/release-preparation.yml' .github/workflows/release-candidate.yml
search_fixed 'operation:' .github/workflows/release-candidate.yml
search_fixed 'recover-preflight' .github/workflows/release-candidate.yml
search_fixed 'candidate_invocation: true' .github/workflows/release-candidate.yml
search_fixed 'recover requires an explicitly supplied target_sha' .github/workflows/release-candidate.yml
ensure_absent '-z "${{ inputs.target_sha }}"' .github/workflows/release-candidate.yml
search_fixed 'refs/notes/release-readiness' .github/workflows/release-candidate.yml
search_fixed 'verification_mode' .github/workflows/release-candidate.yml
search_fixed 'publish": False' .github/workflows/release-candidate.yml
search_fixed 'attestation_sha256' .github/scripts/release_readiness.py
search_fixed 'preparation_manifest_sha256' .github/scripts/release_readiness.py
search_fixed 'candidate_run_id' .github/scripts/release_readiness.py

echo "[contract-check] release queue invariants"
search_fixed 'workflows: ["Release Candidate Pipeline"]' .github/workflows/release.yml
search_fixed 'group: release-main' .github/workflows/release.yml
search_fixed 'release_readiness.py next-ready' .github/workflows/release.yml
search_fixed 'release_readiness.py export' .github/workflows/release.yml
search_fixed 'run-id: ${{ env.PREPARATION_RUN_ID }}' .github/workflows/release.yml
search_fixed 'run-id: ${{ env.SOURCE_GATE_RUN_ID }}' .github/workflows/release.yml
search_fixed 'makeLatest: ${{ needs.prepare.outputs.publish_latest }}' .github/workflows/release.yml
search_fixed 'Create and push tag' .github/workflows/release.yml
search_fixed 'Record release publication ledger' .github/workflows/release.yml
ensure_absent 'release_source_gate.py wait' .github/workflows/release.yml
ensure_absent 'release_preparation.py ensure' .github/workflows/release.yml
ensure_absent 'timeout-seconds: 720' .github/workflows/release.yml
ensure_absent 'poll-seconds: 15' .github/workflows/release.yml
ensure_absent 'recovery_request' .github/workflows/release.yml

echo "[contract-check] reusable workflow and verification-mode invariants"
search_fixed 'workflow_call:' .github/workflows/ci-main.yml
search_fixed 'workflow_call:' .github/workflows/source-build-release-gate.yml
search_fixed 'workflow_call:' .github/workflows/release-preparation.yml
search_fixed 'workflow_dispatch:' .github/workflows/release-preparation.yml
search_fixed 'verification_mode:' .github/workflows/release-preparation.yml
search_fixed 'candidate_invocation:' .github/workflows/release-preparation.yml
search_fixed 'CALLER_WORKFLOW' .github/workflows/release-preparation.yml
search_fixed 'CALLER_WORKFLOW_REF' .github/workflows/release-preparation.yml
search_fixed 'workflow_ref' .github/workflows/ci-main.yml
search_fixed "github.workflow == 'Release Candidate Pipeline'" .github/workflows/ci-main.yml
search_fixed 'strict_fifo=True' .github/scripts/release_readiness.py
search_fixed 'release_enabled_for_missing' .github/scripts/release_readiness.py
search_fixed 'release_enabled_for_commit' .github/scripts/release_snapshot.py
search_fixed 'tag-only publication without complete ledger' .github/scripts/release_readiness.py
search_fixed 'refs/notes/release-overrides' .github/workflows/release-candidate.yml
search_fixed 'not on the main first-parent chain' .github/scripts/release_readiness.py
search_fixed 'candidate-recovery' .github/workflows/release-candidate.yml
search_fixed 'Release Candidate Pipeline' .github/workflows/notify-release-failure.yml
search_fixed 'should_notify' .github/workflows/notify-release-failure.yml
search_fixed "operation in {'push', 'recover'}" .github/workflows/notify-release-failure.yml
search_fixed "workflow_name == 'Release'" .github/workflows/notify-release-failure.yml
ensure_absent 'recovery_request' .github/workflows/release-preparation.yml
ensure_absent 'recovery_request' .github/scripts/release_preparation.py
ensure_absent 'def wait_for_' .github/scripts/release_source_gate.py
ensure_absent 'time.sleep' .github/scripts/release_source_gate.py

echo "[contract-check] existing PR quality-gate anchors"
search_fixed 'cargo test --workspace --locked --all-features -- --test-threads=2' .github/workflows/ci-pr.yml
search_fixed 'Frontend Storybook test (main)' .github/workflows/ci-main.yml
search_fixed 'label-gate' .github/workflows/label-gate.yml
search_fixed 'required_checks' .github/quality-gates.json
search_fixed 'target: runtime' .github/workflows/source-build-release-gate.yml
search_fixed 'target: runtime-supervisor' .github/workflows/source-build-release-gate.yml
search_fixed 'docker/build-push-action@v6' .github/workflows/source-build-release-gate.yml
search_fixed 'DOCKREV_DEPLOY_SMOKE_USE_LOADED_IMAGES: "1"' .github/workflows/source-build-release-gate.yml
search_fixed 'verification_mode: true' .github/workflows/ci-gate-verification.yml
search_fixed 'force_full: true' .github/workflows/ci-gate-verification.yml
search_fixed 'publish": False' .github/workflows/ci-gate-verification.yml
search_fixed 'name: Frontend Storybook test (main)' .github/workflows/ci-main.yml
search_fixed 'Create and push tag' .github/workflows/release.yml
search_fixed 'Create or update GitHub Release + upload assets' .github/workflows/release.yml
search_fixed 'Record release publication ledger' .github/workflows/release.yml
search_fixed 'pending_ready_targets(args) == []' .github/scripts/test_release_readiness.py
search_fixed 'makeLatest: ${{ needs.prepare.outputs.publish_latest }}' .github/workflows/release.yml

ruby -ryaml -e '
workflow = YAML.load_file(".github/workflows/ci-main.yml")
jobs = workflow.fetch("jobs")
required = jobs.fetch("frontend-storybook-test-required")
abort "Storybook required check drifted" unless required.fetch("name") == "Frontend Storybook test (main)"
abort "fast gate must wait for Storybook required check" unless Array(jobs.fetch("fast-gate-verdict").fetch("needs")).include?("frontend-storybook-test-required")
abort "snapshot must wait for Storybook required check" unless Array(jobs.fetch("release-snapshot").fetch("needs")).include?("frontend-storybook-test-required")
'

echo "[contract-check] fixture tests"
python3 .github/scripts/test_release_preparation.py
python3 .github/scripts/test_release_readiness.py
python3 .github/scripts/test_ci_release_gate.py
bash .github/scripts/test-release-snapshot.sh

echo "PASS: release channel contract"
