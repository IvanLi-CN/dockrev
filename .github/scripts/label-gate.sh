#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

labels_json="${LABELS_JSON:-}"
if [[ -z "${labels_json}" && -n "${LABELS_FILE:-}" ]]; then
  labels_json="$(<"${LABELS_FILE}")"
fi
[[ -n "${labels_json}" ]] || { echo "label-gate: LABELS_JSON or LABELS_FILE is required" >&2; exit 2; }

python3 .github/scripts/release_policy.py labels --labels-json "${labels_json}"
