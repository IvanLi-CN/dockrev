#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: deploy-demo-edgeone.sh <demo_dir> <metadata_json>

Publishes one immutable demo artifact directory to the EdgeOne Makers project
bound to DOCKREV_DEMO_EDGEONE_PROJECT. The artifact is built once by `vite
--mode demo`; this script never rebuilds, so the deployed version cannot drift
from the release identity.

Failure handling is evidence-bounded:
  - credential_rejected        explicit 401/403 or token error
  - project_or_permission      explicit project/permission error
  - endpoint_unavailable       transport failure before any remote acceptance
  - deployment_ambiguous       upload/deployment started, remote state unknown
  - auth_or_endpoint_ambiguous insufficient evidence
Automatic retry only covers a complete preflight transport failure. Once an
upload may have happened, retrying could publish twice, so it stops.
EOF
}

if [[ "$#" -ne 2 ]]; then
  usage >&2
  exit 2
fi

demo_dir="$1"
metadata_path="$2"

: "${EDGEONE_API_TOKEN:?repository secret EDGEONE_API_TOKEN is required}"
: "${DOCKREV_DEMO_EDGEONE_PROJECT:?repository secret DOCKREV_DEMO_EDGEONE_PROJECT is required}"

if [[ ! -d "${demo_dir}" ]]; then
  echo "demo artifact directory does not exist: ${demo_dir}" >&2
  exit 1
fi
for required in index.html build-info.json edgeone.json; do
  if [[ ! -f "${demo_dir}/${required}" ]]; then
    echo "demo artifact is missing ${required}" >&2
    exit 1
  fi
done
if [[ -d "${demo_dir}/demo" ]]; then
  echo "demo artifact must not contain a nested demo directory" >&2
  exit 1
fi
if ! python3 - "${metadata_path}" "${demo_dir}/build-info.json" <<'PY'
import json
import sys

metadata_path, build_info_path = sys.argv[1], sys.argv[2]
metadata = json.load(open(metadata_path, encoding="utf-8"))
build_info = json.load(open(build_info_path, encoding="utf-8"))
if build_info.get("variant") != "demo":
    raise SystemExit("demo artifact was not built with the demo variant")
if metadata.get("version") != build_info.get("version"):
    raise SystemExit(
        f"demo artifact version {build_info.get('version')} does not match release version {metadata.get('version')}"
    )
if metadata.get("source_sha") != build_info.get("sourceSha"):
    raise SystemExit(
        f"demo artifact source {build_info.get('sourceSha')} does not match release source {metadata.get('source_sha')}"
    )
PY
then
  echo "demo artifact identity does not match the release identity" >&2
  exit 1
fi

cli_version="${EDGEONE_CLI_VERSION:-1.6.18}"
max_attempts="${EDGEONE_MAX_ATTEMPTS:-3}"
retry_delays_text="${EDGEONE_RETRY_DELAYS_SECONDS:-30 60}"
attempt_logs_dir="$(mktemp -d)"
trap 'rm -rf -- "${attempt_logs_dir}"' EXIT

if ! [[ "${max_attempts}" =~ ^[1-3]$ ]]; then
  echo "EDGEONE_MAX_ATTEMPTS must be an integer from 1 to 3" >&2
  exit 2
fi
read -r -a retry_delays <<< "${retry_delays_text}"
required_delays=$((max_attempts - 1))
if (( ${#retry_delays[@]} < required_delays )); then
  echo "EDGEONE_RETRY_DELAYS_SECONDS must provide at least ${required_delays} delay values" >&2
  exit 2
fi

run_deploy() {
  local output_file="$1"
  set +e
  bunx "edgeone@${cli_version}" makers deploy "${demo_dir}" \
    -n "${DOCKREV_DEMO_EDGEONE_PROJECT}" \
    -e production \
    -t "${EDGEONE_API_TOKEN}" 2>&1 | tee "${output_file}"
  local -a pipeline_status=("${PIPESTATUS[@]}")
  set -e
  if (( pipeline_status[1] != 0 )); then
    echo "EdgeOne CLI succeeded but output capture failed; retry is disabled." >&2
    exit "${pipeline_status[0]}"
  fi
  return "${pipeline_status[0]}"
}

deployment_started() {
  grep -Eiq \
    'Uploading folder to EdgeOne COS|Creating deployment in .* environment|Created deployment with Deployment ID|Waiting for deployment to complete|Deployment in progress|Deploy Success|Deploy URL:|Console URL:' \
    "$1"
}

classify_failure() {
  local output_file="$1"
  if deployment_started "${output_file}"; then
    echo "deployment_ambiguous"
  elif grep -Eiq 'API request failed \((401|403)\)|\b(Unauthorized|AccessDenied|AUTH_[A-Z0-9_]+|TOKEN_[A-Z0-9_]+)\b' "${output_file}"; then
    echo "credential_rejected"
  elif grep -Eiq 'PROJECT_[A-Z0-9_]+|project[[:space:]_-]+(not found|does not exist|permission denied)|invalid project' "${output_file}"; then
    echo "project_or_permission"
  elif grep -Eiq 'AxiosError: timeout|AxiosError: Network Error|ETIMEDOUT|ECONNRESET|ECONNREFUSED|ENOTFOUND|socket hang up|API request failed \((5[0-9]{2})\)' "${output_file}"; then
    echo "endpoint_unavailable"
  else
    echo "auth_or_endpoint_ambiguous"
  fi
}

for ((attempt = 1; attempt <= max_attempts; attempt++)); do
  attempt_log="${attempt_logs_dir}/attempt-${attempt}.log"
  echo "EdgeOne demo deployment attempt ${attempt}/${max_attempts}."
  if run_deploy "${attempt_log}"; then
    echo "EdgeOne demo deployment succeeded."
    exit 0
  else
    status=$?
  fi

  failure_class="$(classify_failure "${attempt_log}")"
  echo "EDGEONE_FAILURE_CLASS=${failure_class}" >&2
  if [[ "${failure_class}" != "endpoint_unavailable" ]]; then
    echo "EdgeOne demo deployment failed before a retryable endpoint failure; automatic retry is disabled." >&2
    exit "${status}"
  fi
  if (( attempt == max_attempts )); then
    echo "EdgeOne demo deployment failed after ${max_attempts} endpoint attempts." >&2
    exit "${status}"
  fi
  delay="${retry_delays[attempt - 1]}"
  echo "Retrying in ${delay}s." >&2
  sleep "${delay}"
done
