#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${DOCKREV_SMOKE_BIN:-}" ]]; then
  echo "DOCKREV_SMOKE_BIN is required" >&2
  exit 1
fi

addr="${DOCKREV_SMOKE_ADDR:-127.0.0.1:50883}"
timeout_seconds="${DOCKREV_SMOKE_TIMEOUT_SECONDS:-20}"

tmp_dir="${RUNNER_TEMP:-/tmp}"
db_path="$(mktemp "${tmp_dir%/}/dockrev.sqlite3.XXXXXXXX")"
log_path="$(mktemp "${tmp_dir%/}/dockrev-smoke.log.XXXXXXXX")"

cleanup() {
  local exit_code="$?"
  if [[ -n "${dockrev_pid:-}" ]]; then
    kill "${dockrev_pid}" >/dev/null 2>&1 || true
    wait "${dockrev_pid}" >/dev/null 2>&1 || true
  fi
  rm -f "${db_path}" "${log_path}" >/dev/null 2>&1 || true
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

base_url="http://${addr}"

export DOCKREV_HTTP_ADDR="${addr}"
export DOCKREV_DB_PATH="${db_path}"

echo "[smoke] starting: ${DOCKREV_SMOKE_BIN} (addr=${addr})"
"${DOCKREV_SMOKE_BIN}" >"${log_path}" 2>&1 &
dockrev_pid="$!"

deadline="$((SECONDS + timeout_seconds))"
while true; do
  if body="$(curl -fsS "${base_url}/api/health" 2>/dev/null)" && [[ "${body}" == "ok" ]]; then
    echo "[smoke] health ok"
    break
  fi

  if (( SECONDS >= deadline )); then
    echo "[smoke] timeout waiting for ${base_url}/api/health" >&2
    echo "[smoke] last logs:" >&2
    tail -n 200 "${log_path}" >&2 || true
    exit 1
  fi

  sleep 0.5
done

html="$(curl -fsS "${base_url}/")"
if ! echo "${html}" | grep -qi "<!doctype html"; then
  echo "[smoke] GET / did not look like HTML (expected <!doctype html>)" >&2
  exit 1
fi
echo "[smoke] ui ok"

if [[ -n "${APP_EFFECTIVE_VERSION:-}" ]]; then
  ver_json="$(curl -fsS "${base_url}/api/version")"
  DOCKREV_SMOKE_JSON="${ver_json}" python3 - <<'PY'
import json
import os
import sys

expected = os.environ.get("APP_EFFECTIVE_VERSION")
data = json.loads(os.environ["DOCKREV_SMOKE_JSON"])
got = data.get("version")
if got != expected:
    print(f"[smoke] version mismatch: got={got} expected={expected}", file=sys.stderr)
    sys.exit(1)
print("[smoke] version ok")
PY
fi

echo "[smoke] passed"
