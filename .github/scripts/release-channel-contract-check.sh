#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

python3 -m json.tool .github/manual-version-release.json >/dev/null
python3 -m json.tool .github/release-failure-notification.json >/dev/null
python3 -m json.tool .github/quality-gates.json >/dev/null
test -s VERSION
version="$(tr -d '[:space:]' < VERSION)"
channel="$(python3 .github/scripts/release_policy.py channel-for-version --version "${version}")"
python3 .github/scripts/release_policy.py validate-channel --version "${version}" --channel "${channel}" >/dev/null
python3 .github/scripts/test_manual_version_release.py
python3 .github/scripts/test_release_workflow_contract.py
python3 .github/scripts/test_workflow_failure_notification_contract.py

for obsolete in \
  ".github/pr-label-release.json" \
  ".github/workflows/label-gate.yml" \
  ".github/scripts/label-gate.sh" \
  "type:patch" \
  "channel:stable" \
  "channel:dev" \
  "version-only-release-pr" \
  "Covered-Product-Merge-SHA"; do
  if rg -n -F "${obsolete}" .github/workflows .github/scripts .github/quality-gates.json \
    --glob '!test_*.py' \
    --glob '!release-channel-contract-check.sh' 2>/dev/null; then
    echo "obsolete release decision reference: ${obsolete}" >&2
    exit 1
  fi
done

echo "PASS: Manual Version Release Delivery contract"
