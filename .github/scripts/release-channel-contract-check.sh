#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

python3 -m json.tool .github/manual-version-release.json >/dev/null
python3 -m json.tool .github/release-failure-notification.json >/dev/null
python3 -m json.tool .github/quality-gates.json >/dev/null
python3 .github/scripts/test_manual_version_release.py
python3 .github/scripts/test_release_workflow_contract.py
python3 .github/scripts/test_workflow_failure_notification_contract.py

for obsolete_path in \
  ".github/workflows/release-preparation.yml" \
  ".github/workflows/release-completion-pr.yml" \
  ".github/workflows/release-abort.yml" \
  ".github/scripts/release_preparation.py" \
  ".github/scripts/release_completion.py" \
  ".github/scripts/release_identity.py" \
  ".github/scripts/release_abort.py" \
  ".github/scripts/release_recovery_artifact.py" \
  ".github/scripts/release_baseline.py" \
  "VERSION" \
  ; do
  if test -e "${obsolete_path}"; then
    echo "obsolete release surface still present: ${obsolete_path}" >&2
    exit 1
  fi
done

for obsolete in \
  "release-reservation/v" \
  "release-publication-lock/v" \
  "release-preparation/v" \
  "Manual Version Release Completion" \
  ".github/pr-label-release.json" \
  "version-only-release-pr" \
  "Covered-Product-Merge-SHA"; do
  if rg -n -F "${obsolete}" .github/workflows .github/scripts \
    --glob '!test_*.py' \
    --glob '!release-channel-contract-check.sh' 2>/dev/null; then
    echo "obsolete release decision reference: ${obsolete}" >&2
    exit 1
  fi
done

echo "PASS: Manual Version Release Delivery contract"
