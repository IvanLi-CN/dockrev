#!/usr/bin/env bash
set -euo pipefail

MAX_ATTEMPTS="${DOCKREV_STORYBOOK_MAX_ATTEMPTS:-2}"
PLAYWRIGHT_INSTALL_TIMEOUT_SEC="${PLAYWRIGHT_INSTALL_TIMEOUT_SEC:-480}"
STORYBOOK_TEST_TIMEOUT_SEC="${STORYBOOK_TEST_TIMEOUT_SEC:-900}"
RETRY_SLEEP_SEC="${RETRY_SLEEP_SEC:-5}"

timestamp() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

run_with_retry() {
  local label="$1"
  local timeout_sec="$2"
  shift 2

  local attempt=1
  while (( attempt <= MAX_ATTEMPTS )); do
    echo "[storybook-ci-check] $(timestamp) START ${label} (attempt ${attempt}/${MAX_ATTEMPTS}, timeout=${timeout_sec}s)"
    if timeout --signal=TERM --kill-after=30s "${timeout_sec}s" "$@"; then
      echo "[storybook-ci-check] $(timestamp) OK ${label} (attempt ${attempt}/${MAX_ATTEMPTS})"
      return 0
    else
      local code=$?
    fi
    if [[ "${code}" -eq 124 ]]; then
      echo "[storybook-ci-check] $(timestamp) TIMEOUT ${label} after ${timeout_sec}s (attempt ${attempt}/${MAX_ATTEMPTS})" >&2
    else
      echo "[storybook-ci-check] $(timestamp) FAIL ${label} with exit ${code} (attempt ${attempt}/${MAX_ATTEMPTS})" >&2
    fi

    if (( attempt == MAX_ATTEMPTS )); then
      echo "[storybook-ci-check] $(timestamp) Giving up ${label}. Check logs above for root cause." >&2
      return "${code}"
    fi

    echo "[storybook-ci-check] $(timestamp) RETRY ${label} after ${RETRY_SLEEP_SEC}s..."
    sleep "${RETRY_SLEEP_SEC}"
    attempt=$((attempt + 1))
  done
}

cd web

mode="${DOCKREV_STORYBOOK_MODE:-full}"
if [[ "${DOCKREV_STORYBOOK_NO_RETRY:-0}" == "1" ]]; then
  MAX_ATTEMPTS=1
fi

# `--with-deps` can hang on GitHub-hosted runners due to apt/dpkg locks.
# The runner already provides the required shared libraries for Chromium.
run_with_retry \
  "playwright install chromium" \
  "${PLAYWRIGHT_INSTALL_TIMEOUT_SEC}" \
  bun ./node_modules/.bin/playwright install chromium

case "${mode}" in
  full)
    run_with_retry \
      "rollback refresh race interaction test" \
      "${STORYBOOK_TEST_TIMEOUT_SEC}" \
      env DOCKREV_TEST_STORYBOOK_ROLLBACK_RACE_ONLY=1 bun run test-storybook
    run_with_retry \
      "storybook interaction tests" \
      "${STORYBOOK_TEST_TIMEOUT_SEC}" \
      bun run test-storybook
    ;;
  global)
    run_with_retry \
      "rollback refresh race interaction test" \
      "${STORYBOOK_TEST_TIMEOUT_SEC}" \
      env DOCKREV_TEST_STORYBOOK_ROLLBACK_RACE_ONLY=1 bun run test-storybook
    run_with_retry \
      "storybook global interaction tests" \
      "${STORYBOOK_TEST_TIMEOUT_SEC}" \
      env DOCKREV_TEST_STORYBOOK_INTERACTIVE_ONLY=1 \
        DOCKREV_STORYBOOK_ROLLBACK_RACE_PASSED=1 bun run test-storybook
    ;;
  shard)
    : "${DOCKREV_TEST_STORYBOOK_SHARD:?DOCKREV_TEST_STORYBOOK_SHARD is required in shard mode}"
    run_with_retry \
      "storybook smoke ${DOCKREV_TEST_STORYBOOK_SHARD}" \
      "${STORYBOOK_TEST_TIMEOUT_SEC}" \
      env DOCKREV_TEST_STORYBOOK_SMOKE_ONLY=1 bun run test-storybook
    ;;
  *)
    echo "Unknown DOCKREV_STORYBOOK_MODE=${mode}; expected full, global, or shard" >&2
    exit 2
    ;;
esac
