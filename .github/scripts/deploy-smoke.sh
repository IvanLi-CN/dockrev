#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

logs_dir="${DOCKREV_DEPLOY_SMOKE_LOG_DIR:-.artifacts/deploy-smoke}"
summary_file="$logs_dir/summary.md"
mkdir -p "$logs_dir"
: > "$summary_file"

append_step_summary() {
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" && -f "$summary_file" ]]; then
    cat "$summary_file" >> "$GITHUB_STEP_SUMMARY"
  fi
}

for tool in docker curl python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Missing required tool: $tool" >&2
    exit 69
  fi
done

if ! docker compose version >/dev/null 2>&1; then
  echo "docker compose is required" >&2
  exit 69
fi

find_free_port() {
  python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
}

wait_for_url() {
  local url="$1"
  local label="$2"
  local timeout_seconds="$3"
  local deadline=$((SECONDS + timeout_seconds))
  local body
  while (( SECONDS < deadline )); do
    if body="$(curl -fsS "$url" 2>/dev/null)"; then
      printf '%s' "$body"
      return 0
    fi
    sleep 1
  done
  echo "Timed out waiting for ${label}: ${url}" >&2
  return 1
}

port="${DOCKREV_DEPLOY_SMOKE_PORT:-$(find_free_port)}"
project="dockrev-smoke-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-0}-$$"
project="${project//[^a-zA-Z0-9_-]/-}"
export DOCKREV_GATEWAY_BIND="127.0.0.1:${port}:80"
compose_file="deploy/docker-compose.yml"
log_file="$logs_dir/docker-compose.log"
cleanup_log="$logs_dir/cleanup.log"
created_deploy_data=0

if [[ ! -d deploy/data ]]; then
  created_deploy_data=1
fi

cleanup() {
  local exit_code="$?"
  {
    echo "[deploy-smoke] docker compose logs"
    docker compose -p "$project" -f "$compose_file" logs --no-color || true
    echo
    echo "[deploy-smoke] docker compose ps"
    docker compose -p "$project" -f "$compose_file" ps || true
  } > "$log_file" 2>&1 || true

  {
    docker compose -p "$project" -f "$compose_file" down -v --remove-orphans || true
    if [[ "$created_deploy_data" == "1" ]]; then
      rm -rf deploy/data || true
    fi
  } > "$cleanup_log" 2>&1 || true

  if (( exit_code != 0 )); then
    echo "[deploy-smoke] failure logs:" >&2
    tail -n 200 "$log_file" >&2 || true
  fi

  append_step_summary
  exit "$exit_code"
}
trap cleanup EXIT

echo "# GitHub-hosted deploy smoke" >> "$summary_file"
echo >> "$summary_file"
echo "- compose_project: $project" | tee -a "$summary_file"
echo "- gateway_bind: $DOCKREV_GATEWAY_BIND" | tee -a "$summary_file"
echo >> "$summary_file"

echo "[deploy-smoke] building runtime images"
mkdir -p deploy/data/supervisor
docker compose -p "$project" -f "$compose_file" build dockrev supervisor

echo "[deploy-smoke] starting deployment topology"
docker compose -p "$project" -f "$compose_file" up -d

base_url="http://127.0.0.1:${port}"
health="$(wait_for_url "$base_url/api/health" "/api/health" 120)"
if [[ "$health" != "ok" ]]; then
  echo "Unexpected /api/health response: $health" >&2
  exit 1
fi

echo "- /api/health: ok" >> "$summary_file"

root_html="$(wait_for_url "$base_url/" "/" 120)"
if ! grep -qi '<!doctype html' <<<"$root_html"; then
  echo "GET / did not return HTML" >&2
  exit 1
fi
if grep -q 'Dockrev Web UI 未构建' <<<"$root_html"; then
  echo "GET / returned placeholder UI instead of built web assets" >&2
  exit 1
fi

echo "- /: built html" >> "$summary_file"

supervisor_html="$(wait_for_url "$base_url/supervisor/" "/supervisor/" 120)"
if ! grep -qi '<!doctype html' <<<"$supervisor_html"; then
  echo "GET /supervisor/ did not return HTML" >&2
  exit 1
fi

echo "- /supervisor/: html" >> "$summary_file"

echo "[deploy-smoke] verifying shared supervisor state mount"
docker compose -p "$project" -f "$compose_file" exec -T supervisor \
  sh -c 'printf "%s\n" "services: {}" > /supervisor-state/self-upgrade.override.yml'
docker compose -p "$project" -f "$compose_file" exec -T dockrev \
  sh -c 'test -r /supervisor-state/self-upgrade.override.yml'
echo "- shared supervisor state: supervisor write and dockrev read" >> "$summary_file"
echo >> "$summary_file"
echo "PASS" | tee -a "$summary_file"
