#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

python3 -m json.tool .github/pr-label-release.json >/dev/null
python3 -m json.tool .github/release-failure-notification.json >/dev/null
python3 -m json.tool .github/quality-gates.json >/dev/null
test -s VERSION
version="$(tr -d '[:space:]' < VERSION)"
channel="$(python3 .github/scripts/release_policy.py channel-for-version --version "${version}")"
python3 .github/scripts/release_policy.py validate-channel --version "${version}" --channel "${channel}"
python3 .github/scripts/test_pr_label_release.py
python3 .github/scripts/test_release_workflow_contract.py
rg -q 'release-recovery' .github/scripts/release_preparation.py .github/scripts/release_completion.py .github/scripts/release_identity.py

for obsolete in \
  'Release Candidate Pipeline' \
  'release_snapshot.py' \
  'release_readiness.py' \
  'release_source_gate.py' \
  'refs/notes/release-snapshots' \
  'refs/notes/release-readiness' \
  'next-ready' \
  'release queue'; do
  if rg -n -F "${obsolete}" .github/workflows .github/scripts README.md docs/release.md \
    --glob '!release-channel-contract-check.sh' \
    --glob '!test_release_workflow_contract.py' 2>/dev/null; then
    echo "obsolete release contract reference: ${obsolete}" >&2
    exit 1
  fi
done

echo "PASS: PR label release contract"
