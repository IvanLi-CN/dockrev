#!/usr/bin/env bash
set -euo pipefail

scenario="${TESTBOX_E2E_SCENARIO:-all}"
repeat_count="${TESTBOX_E2E_REPEAT:-1}"
logs_dir="${TESTBOX_E2E_LOGS_DIR:-.artifacts/testbox-e2e}"
summary_file="${logs_dir}/summary.md"

mkdir -p "$logs_dir"
: > "$summary_file"

append_step_summary() {
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" && -f "$summary_file" ]]; then
    cat "$summary_file" >> "$GITHUB_STEP_SUMMARY"
  fi
}

trap append_step_summary EXIT

if ! [[ "$repeat_count" =~ ^[1-9][0-9]*$ ]]; then
  echo "TESTBOX_E2E_REPEAT must be a positive integer, got: $repeat_count" >&2
  exit 64
fi

if (( repeat_count > 3 )); then
  echo "TESTBOX_E2E_REPEAT must be <= 3 to avoid overloading the shared testbox, got: $repeat_count" >&2
  exit 64
fi

case "$scenario" in
  all)
    scripts=(
      "full-deploy-smoke.e2e.ts"
      "check-job-recovery.e2e.ts"
      "check-version-inference-sse.e2e.ts"
      "check-service-update-no-semver-pull.e2e.ts"
    )
    ;;
  full-deploy-smoke)
    scripts=("full-deploy-smoke.e2e.ts")
    ;;
  check-job-recovery)
    scripts=("check-job-recovery.e2e.ts")
    ;;
  check-version-inference-sse)
    scripts=("check-version-inference-sse.e2e.ts")
    ;;
  check-service-update-no-semver-pull)
    scripts=("check-service-update-no-semver-pull.e2e.ts")
    ;;
  *)
    echo "Unsupported TESTBOX_E2E_SCENARIO: $scenario" >&2
    exit 64
    ;;
esac

for tool in ssh rsync bun; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Missing required tool: $tool" >&2
    exit 69
  fi
done

if [[ -n "${TESTBOX_HOST:-}" ]]; then
  echo "Using TESTBOX_HOST=$TESTBOX_HOST"
else
  echo "Using TESTBOX_HOST=codex-testbox (script default)"
fi

echo "# codex-testbox E2E" >> "$summary_file"
echo >> "$summary_file"
echo "- scenario: $scenario" | tee -a "$summary_file"
echo "- repeat_count: $repeat_count" | tee -a "$summary_file"
echo >> "$summary_file"

overall_status=0

run_script() {
  local script_name="$1"
  local iteration="$2"
  local stem="${script_name%.e2e.ts}"
  local log_file="$logs_dir/${stem}.run-${iteration}.log"

  echo "::group::${stem} (run ${iteration}/${repeat_count})"
  echo "## ${stem} (run ${iteration}/${repeat_count})" >> "$summary_file"
  local status=0
  set +e
  bun "scripts/testbox/${script_name}" 2>&1 | tee "$log_file"
  status=${PIPESTATUS[0]}
  set -e
  echo "- exit_code: ${status}" >> "$summary_file"
  echo "- log: ${log_file}" >> "$summary_file"
  echo >> "$summary_file"
  echo "::endgroup::"

  if (( status != 0 )); then
    overall_status=$status
    return $status
  fi

  return 0
}

for script_name in "${scripts[@]}"; do
  for ((iteration = 1; iteration <= repeat_count; iteration++)); do
    if ! run_script "$script_name" "$iteration"; then
      :
    fi
  done
done

exit "$overall_status"
