#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/verify_shared_testbox_compose_v2.sh [--keep-run] [--run-id RUN_ID] [--testbox HOST] [--json-out PATH]

Run real Compose V2 lifecycle and admission checks on codex-testbox.

Options:
  --keep-run        Keep the isolated remote run for manual inspection.
  --run-id ID       Override the generated run id.
  --testbox HOST    SSH alias/host to use (default: codex-testbox).
  --json-out PATH   Write the final summary JSON to a local file.
  -h, --help        Show this help.
USAGE
}

KEEP_RUN=0
TESTBOX="codex-testbox"
RUN_ID=""
JSON_OUT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --keep-run)
      KEEP_RUN=1
      shift
      ;;
    --run-id)
      RUN_ID="${2:?missing value for --run-id}"
      shift 2
      ;;
    --testbox)
      TESTBOX="${2:?missing value for --testbox}"
      shift 2
      ;;
    --json-out)
      JSON_OUT="${2:?missing value for --json-out}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 2
  }
}

[[ "$TESTBOX" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,253}$ ]] || {
  echo "testbox must be a single safe SSH host or alias" >&2
  exit 2
}

for command in base64 git ssh rsync python3; do
  require_cmd "$command"
done

if REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  :
else
  REPO_ROOT="$(pwd)"
fi
REPO_ROOT="$(python3 - "$REPO_ROOT" <<'PY'
import os, sys
print(os.path.realpath(sys.argv[1]))
PY
)"
REPO_NAME="$(basename "$REPO_ROOT")"
REMOTE_USER="${USER:-}"
[[ "$REMOTE_USER" =~ ^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$ ]] || {
  echo "USER must be a single safe slug for shared testbox runs" >&2
  exit 2
}
REPO_WORKSPACE_SLUG="$(python3 - "$REPO_NAME" <<'PY'
import re, sys

value = re.sub(r'[^a-z0-9_-]+', '_', sys.argv[1].lower()).strip('_')
print(value or 'repo')
PY
)"
PATH_HASH8="$(python3 - "$REPO_ROOT" <<'PY'
import hashlib, os, sys
print(hashlib.sha256(os.path.realpath(sys.argv[1]).encode()).hexdigest()[:8])
PY
)"
GIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo nogit)"
if [[ -z "$RUN_ID" ]]; then
  RUN_ID="$(date -u +%Y%m%d_%H%M%S)_${GIT_SHA}_$(python3 -c 'import secrets; print(secrets.token_hex(4))')"
fi
[[ "$RUN_ID" =~ ^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$ ]] || {
  echo "run id must be a single safe slug (letters, digits, _ or -)" >&2
  exit 2
}
WORKSPACE_SLUG="${REPO_WORKSPACE_SLUG}__${PATH_HASH8}"
REMOTE_BASE="/srv/codex/workspaces/$REMOTE_USER"
REMOTE_WORKSPACE="${REMOTE_BASE}/${WORKSPACE_SLUG}"
REMOTE_RUN="${REMOTE_WORKSPACE}/runs/${RUN_ID}"

compose_project_slug() {
  python3 - "$1" "$2" "$3" <<'PY'
import hashlib, re, sys

prefix, repo_name, run_id = sys.argv[1:4]

def slug(value: str) -> str:
    value = re.sub(r'[^a-z0-9_-]+', '_', value.lower()).strip('_')
    return value or "x"

prefix_slug = slug(prefix)
repo_slug = slug(repo_name)
run_slug = slug(run_id)
entropy = hashlib.sha256(f"{prefix_slug}:{repo_name}:{run_id}".encode()).hexdigest()[:8]
# Reserve one repository character and the entropy suffix so Compose's
# project name stays within Docker's portable 63-character limit.
max_run_len = max(1, 63 - len(prefix_slug) - 2 - 1 - 1 - len(entropy))
run_slug = run_slug[:max_run_len]
suffix = f"{run_slug}_{entropy}"
max_repo_len = max(1, 63 - len(prefix_slug) - len(suffix) - 2)
print(f"{prefix_slug}_{repo_slug[:max_repo_len]}_{suffix}")
PY
}

FIXTURE_PROJECT="$(compose_project_slug "composev2" "$REPO_NAME" "$RUN_ID")"
[[ ${#FIXTURE_PROJECT} -le 63 ]] || {
  echo "generated Compose project exceeds the portable 63-character limit" >&2
  exit 2
}
SUMMARY_TMP="$(mktemp -t dockrev-testbox-compose-v2-summary.XXXXXX.json)"

base64_encode() {
  printf '%s' "$1" | base64 | tr -d '\n'
}

REMOTE_RUN_B64="$(base64_encode "$REMOTE_RUN")"
REMOTE_BASE_B64="$(base64_encode "$REMOTE_BASE")"
REMOTE_WORKSPACE_B64="$(base64_encode "$REMOTE_WORKSPACE")"
RUN_ID_B64="$(base64_encode "$RUN_ID")"
FIXTURE_PROJECT_B64="$(base64_encode "$FIXTURE_PROJECT")"
REPO_ROOT_B64="$(base64_encode "$REPO_ROOT")"
SYNC_EXCLUDES=(
  --exclude '.git/'
  --exclude 'node_modules/'
  --exclude 'target/'
  --exclude 'dist/'
  --exclude 'build/'
  --exclude '.next/'
  --exclude '.venv/'
)

printf '==> Preparing remote workspace %s on %s\n' "$REMOTE_RUN" "$TESTBOX"
ssh -o BatchMode=yes -- "$TESTBOX" \
  "env REMOTE_RUN_B64=$REMOTE_RUN_B64 REMOTE_BASE_B64=$REMOTE_BASE_B64 REMOTE_WORKSPACE_B64=$REMOTE_WORKSPACE_B64 RUN_ID_B64=$RUN_ID_B64 REPO_ROOT_B64=$REPO_ROOT_B64 bash -s" <<'REMOTE_SETUP'
set -euo pipefail

command -v base64 >/dev/null 2>&1 || {
  echo "missing required remote command: base64" >&2
  exit 2
}

decode_base64() {
  printf '%s' "$1" | base64 -d
}

REMOTE_RUN="$(decode_base64 "$REMOTE_RUN_B64")"
REMOTE_BASE="$(decode_base64 "$REMOTE_BASE_B64")"
REMOTE_WORKSPACE="$(decode_base64 "$REMOTE_WORKSPACE_B64")"
RUN_ID="$(decode_base64 "$RUN_ID_B64")"
REPO_ROOT="$(decode_base64 "$REPO_ROOT_B64")"
ensure_directory() {
  local path="$1"
  if [[ -e "$path" ]]; then
    [[ -d "$path" && ! -L "$path" ]] || {
      echo "remote directory is unsafe: $path" >&2
      exit 2
    }
  else
    mkdir -- "$path"
  fi
}

for path in /srv/codex /srv/codex/workspaces; do
  [[ -d "$path" && ! -L "$path" ]] || {
    echo "shared testbox root is unsafe: $path" >&2
    exit 2
  }
done
case "$REMOTE_BASE" in
  /srv/codex/workspaces/*) ;;
  *) echo "remote base escaped /srv/codex/workspaces" >&2; exit 2 ;;
esac
case "$REMOTE_WORKSPACE" in
  "$REMOTE_BASE"/*) ;;
  *) echo "remote workspace escaped the expected base" >&2; exit 2 ;;
esac
case "$REMOTE_RUN" in
  "$REMOTE_WORKSPACE/runs/$RUN_ID") ;;
  *) echo "remote run path escaped the expected workspace" >&2; exit 2 ;;
esac
ensure_directory "$REMOTE_BASE"
ensure_directory "$REMOTE_WORKSPACE"
ensure_directory "$REMOTE_WORKSPACE/runs"
if ! mkdir "$REMOTE_RUN"; then
  echo "remote run already exists: $REMOTE_RUN" >&2
  exit 2
fi
printf '%s\n' "local_repo_root=$REPO_ROOT" "created_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)" "last_run_id=$RUN_ID" > "$REMOTE_WORKSPACE/workspace.txt"
REMOTE_SETUP

printf '==> Syncing repository to shared testbox\n'
rsync -azs --delete "${SYNC_EXCLUDES[@]}" -- "$REPO_ROOT/" "$TESTBOX:$REMOTE_RUN/"

printf '==> Running real Compose V2 regression on shared testbox\n'
ssh -o BatchMode=yes -- "$TESTBOX" \
  "env REMOTE_RUN_B64=$REMOTE_RUN_B64 REMOTE_BASE_B64=$REMOTE_BASE_B64 REMOTE_WORKSPACE_B64=$REMOTE_WORKSPACE_B64 RUN_ID_B64=$RUN_ID_B64 FIXTURE_PROJECT_B64=$FIXTURE_PROJECT_B64 KEEP_RUN=$KEEP_RUN bash -s" \
  > "$SUMMARY_TMP" <<'REMOTE_SCRIPT'
set -euo pipefail

decode_base64() {
  printf '%s' "$1" | base64 -d
}

REMOTE_RUN="$(decode_base64 "$REMOTE_RUN_B64")"
REMOTE_BASE="$(decode_base64 "$REMOTE_BASE_B64")"
REMOTE_WORKSPACE="$(decode_base64 "$REMOTE_WORKSPACE_B64")"
RUN_ID="$(decode_base64 "$RUN_ID_B64")"
FIXTURE_PROJECT="$(decode_base64 "$FIXTURE_PROJECT_B64")"

verify_remote_scope() {
  case "$REMOTE_BASE" in
    /srv/codex/workspaces/*) ;;
    *) return 1 ;;
  esac
  case "$REMOTE_WORKSPACE" in
    "$REMOTE_BASE"/*) ;;
    *) return 1 ;;
  esac
  case "$REMOTE_RUN" in
    "$REMOTE_WORKSPACE/runs/$RUN_ID") ;;
    *) return 1 ;;
  esac
  for path in /srv/codex /srv/codex/workspaces "$REMOTE_BASE" "$REMOTE_WORKSPACE" "$REMOTE_WORKSPACE/runs" "$REMOTE_RUN"; do
    [[ -d "$path" && ! -L "$path" ]] || return 1
  done
}

verify_remote_scope || {
  echo "remote run scope is unsafe" >&2
  exit 2
}
cd "$REMOTE_RUN"
mkdir -p fixture bin artifacts
exec 2> >(tee -a artifacts/remote-test.log >&2)

log() {
  printf ':: %s\n' "$*" >&2
}

json_get() {
  local path="$1"
  local input="$2"
  python3 - "$path" "$input" <<'PY'
import json, sys
current = json.loads(sys.argv[2])
for part in [value for value in sys.argv[1].split('.') if value]:
    current = current[int(part)] if part.isdigit() else current[part]
if isinstance(current, (dict, list)):
    print(json.dumps(current, ensure_ascii=False))
else:
    print(current)
PY
}

json_assert() {
  local expression="$1"
  local input="$2"
  python3 - "$expression" "$input" <<'PY'
import json, sys
expression, raw = sys.argv[1:3]
payload = json.loads(raw)
parts = [value for value in expression.split('.') if value]
current = payload
for part in parts:
    current = current[int(part)] if part.isdigit() else current[part]
if not current:
    raise SystemExit(f"JSON assertion failed: {expression} in {raw}")
PY
}

curl_body() {
  local method="$1"
  local path="$2"
  local body="${3:-}"
  local url="http://127.0.0.1:${PORT}${path}"
  if [[ -n "$body" ]]; then
    curl --silent --show-error --fail-with-body \
      -H 'content-type: application/json' \
      -X "$method" "$url" --data "$body"
  else
    curl --silent --show-error --fail-with-body \
      -H 'content-type: application/json' \
      -X "$method" "$url"
  fi
}

curl_status_body() {
  local method="$1"
  local path="$2"
  local body="${3:-}"
  local url="http://127.0.0.1:${PORT}${path}"
  if [[ -n "$body" ]]; then
    curl --silent --show-error -H 'content-type: application/json' \
      -X "$method" "$url" --data "$body" -w '\n%{http_code}'
  else
    curl --silent --show-error -H 'content-type: application/json' \
      -X "$method" "$url" -w '\n%{http_code}'
  fi
}

wait_http() {
  local url="$1"
  for _ in $(seq 1 60); do
    if curl --silent --show-error --fail "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "timed out waiting for $url" >&2
  return 1
}

reserve_port() {
  python3 - <<'PY'
import socket
sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
}

poll_job() {
  local job_id="$1"
  local payload
  for _ in $(seq 1 120); do
    payload="$(curl_body GET "/api/jobs/${job_id}")"
    case "$(json_get job.status "$payload")" in
      queued|running)
        sleep 1
        ;;
      *)
        printf '%s' "$payload"
        return 0
        ;;
    esac
  done
  echo "timed out waiting for job $job_id" >&2
  return 1
}

wait_deploy_report() {
  local payload
  for _ in $(seq 1 90); do
    payload="$(curl_body GET /api/deploy-check/report)"
    if python3 - "$payload" <<'PY'
import json, sys
payload = json.loads(sys.argv[1])
if payload.get("status") != "ready" or payload.get("refreshing") is not False:
    raise SystemExit(1)
if payload.get("lastError"):
    raise SystemExit("deploy-check refresh failed: " + str(payload["lastError"]))
PY
    then
      printf '%s' "$payload"
      return 0
    fi
    sleep 1
  done
  echo "timed out waiting for deploy-check report" >&2
  return 1
}

request_deploy_check_refresh() {
  curl_body POST /api/deploy-check/report/refresh >/dev/null
}

wait_for_compose_access_status() {
  local expected_status="$1"
  local payload
  for _ in $(seq 1 90); do
    payload="$(curl_body GET /api/deploy-check/report)"
    if python3 - "$expected_status" "$payload" <<'PY'
import json, sys

expected, raw = sys.argv[1:3]
payload = json.loads(raw)
if payload.get("status") != "ready" or payload.get("refreshing") is not False:
    raise SystemExit(1)
if payload.get("lastError"):
    raise SystemExit("deploy-check refresh failed: " + str(payload["lastError"]))
for item in payload.get("report", {}).get("checks", []):
    if item.get("id") == "core.compose_access":
        if item.get("status") == expected:
            raise SystemExit(0)
        raise SystemExit(1)
raise SystemExit("core.compose_access missing from deploy-check report")
PY
    then
      printf '%s' "$payload"
      return 0
    fi
    sleep 1
  done
  echo "timed out waiting for core.compose_access=${expected_status}" >&2
  return 1
}

generate_caps_override() {
  local output="$1"
  local services
  services="$(docker compose -p "$FIXTURE_PROJECT" -f fixture/compose.yaml config --services)"
  {
    echo 'services:'
    while IFS= read -r service; do
      [[ -n "$service" ]] || continue
      cat <<YAML
  $service:
    cap_drop:
      - ALL
    cap_add:
      - CHOWN
      - DAC_OVERRIDE
      - FSETID
      - FOWNER
      - MKNOD
      - NET_RAW
      - SETGID
      - SETUID
      - SETPCAP
      - NET_BIND_SERVICE
      - SYS_CHROOT
      - KILL
      - AUDIT_WRITE
YAML
    done <<< "$services"
  } > "$output"
}

compose_fixture() {
  docker compose -p "$FIXTURE_PROJECT" -f fixture/compose.yaml -f fixture/.codex.caps.yaml "$@"
}

discover_fixture() {
  local discovery job_id project_payload stack_payload
  discovery="$(curl_body POST /api/discovery/scan '{}')"
  job_id="$(json_get jobId "$discovery")"
  local job
  job="$(poll_job "$job_id")"
  [[ "$(json_get job.status "$job")" == "success" ]] || {
    echo "discovery failed: $job" >&2
    return 1
  }
  project_payload="$(curl_body GET /api/discovery/projects)"
  stack_payload="$(python3 - "$FIXTURE_PROJECT" "$project_payload" <<'PY'
import json, sys
project, payload = sys.argv[1], json.loads(sys.argv[2])
for item in payload.get("projects", []):
    if item.get("project") == project:
        print(item["stackId"])
        break
else:
    raise SystemExit(f"discovered project not found: {project}")
PY
)"
  printf '%s' "$stack_payload"
}

start_server() {
  local compose_bin="$1"
  PORT="$(reserve_port)"
  DB_PATH="$REMOTE_RUN/artifacts/${MODE}.sqlite3"
  LOG_PATH="$REMOTE_RUN/artifacts/${MODE}.log"
  local dockrev_path="$PATH"
  if [[ "$MODE" == "plugin" ]]; then
    dockrev_path="$REMOTE_RUN/bin:$PATH"
  fi
  DOCKREV_HTTP_ADDR="127.0.0.1:${PORT}" \
    DOCKREV_DB_PATH="$DB_PATH" \
    DOCKREV_COMPOSE_BIN="$compose_bin" \
    DOCKREV_COMMAND_LOG="$COMMAND_LOG" \
    DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=true \
    PATH="$dockrev_path" \
    "$REMOTE_RUN/target/release/dockrev" >"$LOG_PATH" 2>&1 &
  PID=$!
  wait_http "http://127.0.0.1:${PORT}/api/health"
}

stop_server() {
  if [[ -n "${PID:-}" ]]; then
    kill "$PID" >/dev/null 2>&1 || true
    wait "$PID" >/dev/null 2>&1 || true
    PID=""
  fi
}

cleanup_remote() {
  set +e
  stop_server
  if ! verify_remote_scope; then
    echo "refusing cleanup outside the verified shared-testbox run scope" >&2
    return
  fi
  cd "$REMOTE_WORKSPACE/runs" || return
  [[ "$(pwd -P)" == "$REMOTE_WORKSPACE/runs" ]] || {
    echo "refusing cleanup from an unexpected remote runs directory" >&2
    return
  }
  [[ -d "$RUN_ID" && ! -L "$RUN_ID" ]] || {
    echo "refusing cleanup for a missing or symlinked remote run" >&2
    return
  }
  cd "$RUN_ID" || return
  [[ "$(pwd -P)" == "$REMOTE_RUN" ]] || {
    echo "refusing cleanup from an unexpected remote run directory" >&2
    return
  }
  compose_fixture down -v --remove-orphans >/dev/null 2>&1 || true
  if [[ "$KEEP_RUN" != "1" ]]; then
    # Stay relative to the verified run directory so a renamed parent cannot
    # redirect cleanup through a newly introduced absolute-path symlink.
    cd .. || return
    rm -rf -- "$RUN_ID"
  fi
}
trap cleanup_remote EXIT

cat > fixture/compose.yaml <<'YAML'
services:
  app:
    image: alpine:3.20
    command: ["sh", "-c", "while true; do sleep 3600; done"]
YAML
generate_caps_override fixture/.codex.caps.yaml

log "checking real Compose plugin and standalone versions"
plugin_version="$(docker compose -p "$FIXTURE_PROJECT" version)"
standalone_version="$(docker-compose -p "$FIXTURE_PROJECT" version)"
plugin_major="$(python3 - "$plugin_version" <<'PY'
import re, sys
match = re.search(r'(?m)^Docker Compose version v?(\d+)', sys.argv[1])
print(match.group(1) if match else "")
PY
)"
standalone_major="$(python3 - "$standalone_version" <<'PY'
import re, sys
match = re.search(r'(?m)^Docker Compose version v?(\d+)', sys.argv[1])
print(match.group(1) if match else "")
PY
)"
[[ "$plugin_major" -ge 2 && "$standalone_major" -ge 2 ]] || {
  echo "testbox does not provide Compose V2 plugin and standalone" >&2
  exit 1
}

log "building current Dockrev binary on testbox"
cargo build -p dockrev-api --bin dockrev --release --locked >&2

cat > bin/docker-compose-v1 <<'SH'
#!/bin/sh
if [ "${1:-}" = "version" ]; then
  printf '%s\n' 'docker-compose version 1.29.2, build regression-fixture'
  exit 0
fi
printf '%s\n' "$*" >> "${DOCKREV_V1_LOG:?}"
exec docker compose "$@"
SH
chmod +x bin/docker-compose-v1

REAL_DOCKER="$(command -v docker)"
REAL_STANDALONE="$(command -v docker-compose)"
cat > bin/docker <<SH
#!/bin/sh
printf '%s\\n' "\$*" >> "\${DOCKREV_COMMAND_LOG:?}"
exec "$REAL_DOCKER" "\$@"
SH
cat > bin/docker-compose-v2 <<SH
#!/bin/sh
printf '%s\\n' "\$*" >> "\${DOCKREV_COMMAND_LOG:?}"
exec "$REAL_STANDALONE" "\$@"
SH
chmod +x bin/docker bin/docker-compose-v2

declare -a RESULTS=()

run_v2_mode() {
  MODE="$1"
  COMPOSE_BIN="$2"
  COMMAND_LOG="$REMOTE_RUN/artifacts/${MODE}-compose.log"
  : > "$COMMAND_LOG"
  log "running ${MODE} real lifecycle test with ${COMPOSE_BIN}"
  compose_fixture up -d >&2
  start_server "$COMPOSE_BIN"

  report="$(wait_deploy_report)"
  executor_status="$(python3 - "$report" <<'PY'
import json, sys
report = json.loads(sys.argv[1])
for item in report["report"]["checks"]:
    if item["id"] == "core.update_executor_ready":
        print(item["status"])
        break
else:
    raise SystemExit("core.update_executor_ready missing")
PY
)"
  [[ "$executor_status" == "pass" ]] || {
    echo "${MODE} deploy-check executor was not pass: $report" >&2
    return 1
  }

  stack_id="$(discover_fixture)"
  service_payload="$(curl_body GET "/api/stacks/${stack_id}")"
  service_id="$(json_get stack.services.0.id "$service_payload")"
  compose_fixture down -v --remove-orphans >&2
  [[ -z "$(compose_fixture ps -a -q)" ]] || {
    echo "${MODE} fixture still has containers after down" >&2
    return 1
  }

  status_payload="$(curl_body GET "/api/services/${service_id}/lifecycle-status")"
  [[ "$(json_get state "$status_payload")" == "stopped" ]] || {
    echo "${MODE} empty compose project was not stopped: $status_payload" >&2
    return 1
  }

  start_payload="$(curl_body POST "/api/services/${service_id}/lifecycle" '{"action":"start"}')"
  job_payload="$(poll_job "$(json_get jobId "$start_payload")")"
  [[ "$(json_get job.status "$job_payload")" == "success" ]] || {
    echo "${MODE} lifecycle start failed: $job_payload" >&2
    return 1
  }
  [[ -n "$(compose_fixture ps -q app)" ]] || {
    echo "${MODE} lifecycle start did not create a running container" >&2
    return 1
  }
  running_status="$(curl_body GET "/api/services/${service_id}/lifecycle-status")"
  [[ "$(json_get state "$running_status")" == "running" ]] || {
    echo "${MODE} lifecycle start did not report running: $running_status" >&2
    return 1
  }
  lifecycle_command="$(grep -E '(^| )up -d --pull never --no-recreate --no-deps app($| )' "$COMMAND_LOG" | tail -n 1 || true)"
  [[ -n "$lifecycle_command" ]] || {
    echo "${MODE} did not execute the expected no-pull/no-recreate/no-deps command" >&2
    cat "$COMMAND_LOG" >&2
    return 1
  }

  RESULTS+=("$(python3 - "$MODE" "$COMPOSE_BIN" "$plugin_version" "$standalone_version" "$executor_status" "$stack_id" "$service_id" "$status_payload" "$job_payload" "$running_status" "$lifecycle_command" <<'PY'
import json, sys
mode, compose_bin, plugin_version, standalone_version, executor_status, stack_id, service_id, stopped, job, running, lifecycle_command = sys.argv[1:]
print(json.dumps({
    "mode": mode,
    "composeBin": compose_bin,
    "pluginVersion": plugin_version,
    "standaloneVersion": standalone_version,
    "executorStatus": executor_status,
    "stackId": stack_id,
    "serviceId": service_id,
    "emptyPsState": json.loads(stopped)["state"],
    "startJobStatus": json.loads(job)["job"]["status"],
    "runningState": json.loads(running)["state"],
    "lifecycleCommand": lifecycle_command,
    "noPullLifecycle": "--pull never" in lifecycle_command,
    "noRecreateLifecycle": "--no-recreate" in lifecycle_command,
    "noDepsLifecycle": "--no-deps" in lifecycle_command,
}, sort_keys=True))
PY
)")
  stop_server
  compose_fixture down -v --remove-orphans >&2
}

run_v1_mode() {
  MODE="compose-v1-rejected"
  V1_LOG="$REMOTE_RUN/artifacts/compose-v1-wrapper.log"
  : > "$V1_LOG"
  export DOCKREV_V1_LOG="$V1_LOG"
  log "running Compose V1 rejection test"
  compose_fixture up -d >&2
  start_server "$REMOTE_RUN/bin/docker-compose-v1"
  report="$(wait_deploy_report)"
  executor_status="$(python3 - "$report" <<'PY'
import json, sys
report = json.loads(sys.argv[1])
for item in report["report"]["checks"]:
    if item["id"] == "core.update_executor_ready":
        if item["status"] != "fail" or item["evidence"].find("compose_v2_required") < 0:
            raise SystemExit(f"unexpected executor check: {item}")
        print(item["status"])
        break
else:
    raise SystemExit("core.update_executor_ready missing")
PY
)"
  stack_id="$(discover_fixture)"
  service_payload="$(curl_body GET "/api/stacks/${stack_id}")"
  service_id="$(json_get stack.services.0.id "$service_payload")"
  compose_fixture down -v --remove-orphans >&2
  before_containers="$(compose_fixture ps -a -q)"
  response_with_status="$(curl_status_body POST "/api/services/${service_id}/lifecycle" '{"action":"start"}')"
  response_body="$(sed '$d' <<< "$response_with_status")"
  response_status="$(tail -n 1 <<< "$response_with_status")"
  [[ "$response_status" == "503" ]] || {
    echo "V1 lifecycle gate returned HTTP $response_status: $response_body" >&2
    return 1
  }
  [[ "$(json_get error.code "$response_body")" == "compose_v2_required" ]] || {
    echo "V1 lifecycle gate returned wrong code: $response_body" >&2
    return 1
  }
  after_containers="$(compose_fixture ps -a -q)"
  [[ "$before_containers" == "$after_containers" ]] || {
    echo "V1 lifecycle gate changed containers: before=$before_containers after=$after_containers" >&2
    return 1
  }
  if grep -Eqi '(^| )(up|create|start|stop|restart)( |$)' "$V1_LOG"; then
    echo "V1 lifecycle gate invoked a mutating Compose command: $(cat "$V1_LOG")" >&2
    return 1
  fi
  RESULTS+=("$(python3 - "$executor_status" "$response_status" "$before_containers" "$after_containers" "$V1_LOG" <<'PY'
import json, sys
executor_status, response_status, before, after, log_path = sys.argv[1:]
with open(log_path, encoding="utf-8") as stream:
    commands = [line.strip() for line in stream if line.strip()]
print(json.dumps({
    "mode": "compose-v1-rejected",
    "executorStatus": executor_status,
    "writeHttpStatus": int(response_status),
    "writeErrorCode": "compose_v2_required",
    "containersBeforeWrite": before,
    "containersAfterWrite": after,
    "wrapperCommands": commands,
    "mutatingComposeCommandObserved": False,
}, sort_keys=True))
PY
)")
  stop_server
  compose_fixture down -v --remove-orphans >&2
}

run_persisted_discovery_reconciliation() {
  MODE="persisted-discovery-reconciliation"
  COMMAND_LOG="$REMOTE_RUN/artifacts/${MODE}-compose.log"
  : > "$COMMAND_LOG"
  log "running persisted Compose discovery reconciliation test"
  compose_fixture up -d >&2
  start_server docker

  healthy_stack_id="$(discover_fixture)"
  request_deploy_check_refresh
  healthy_report="$(wait_for_compose_access_status pass)"
  before_containers="$(compose_fixture ps -a -q)"
  before_running_containers="$(compose_fixture ps -q)"
  stopped_compose_path="$REMOTE_RUN/fixture/stopped-compose.yaml"
  missing_compose_path="$REMOTE_RUN/fixture/missing-compose.yaml"
  invalid_compose_path="$REMOTE_RUN/fixture/invalid-compose.yaml"
  manual_compose_path="$REMOTE_RUN/fixture/manual-stopped-compose.yaml"

  python3 - "$DB_PATH" "$stopped_compose_path" "$missing_compose_path" "$invalid_compose_path" "$manual_compose_path" <<'PY'
import json, sqlite3, sys

db_path, stopped_path, missing_path, invalid_path, manual_path = sys.argv[1:6]
now = "2026-08-12T00:00:00Z"
with open(stopped_path, "w", encoding="utf-8") as stream:
    stream.write("services:\n  app:\n    image: alpine:3.20\n")
with open(invalid_path, "w", encoding="utf-8") as stream:
    stream.write("services: [invalid\n")
with open(manual_path, "w", encoding="utf-8") as stream:
    stream.write("services:\n  app:\n    image: alpine:3.20\n")

conn = sqlite3.connect(db_path)
try:
    projects = [
        ("legacy-auto-stopped", stopped_path, 1, "auto_archive_on_restart"),
        ("missing-compose", missing_path, 0, None),
        ("invalid-auto", invalid_path, 1, "auto_archive_on_restart"),
        ("manual-stopped", manual_path, 1, "user_archive"),
    ]
    for project, path, archived, reason in projects:
        stack_id = f"{project}-stack"
        conn.execute(
            """
            INSERT INTO stacks (
              id, name, compose_type, compose_files_json, backup_targets_json,
              backup_retention_keep_last, backup_retention_delete_after_stable_seconds,
              archived, archived_at, archived_reason, created_at, updated_at, last_check_at
            ) VALUES (?, ?, 'path', ?, '[]', 0, 0, ?, ?, ?, ?, ?, ?)
            """,
            (stack_id, stack_id, json.dumps([path]), archived, now if archived else None, reason, now, now, now),
        )
        conn.execute(
            """
            INSERT INTO discovered_compose_projects (
              project, stack_id, status, last_config_files_json, archived, archived_at, archived_reason
            ) VALUES (?, ?, 'active', ?, ?, ?, ?)
            """,
            (project, stack_id, json.dumps([path]), archived, now if archived else None, reason),
        )
    conn.execute(
        """
        INSERT INTO services (
          id, stack_id, name, image_ref, image_tag, auto_rollback,
          backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at
        ) VALUES (
          'legacy-auto-stopped-service', 'legacy-auto-stopped-stack', 'app', 'alpine:3.20', '3.20', 0,
          '["/srv/legacy/data"]', '["legacy_data"]', ?, ?
        )
        """,
        (now, now),
    )
    conn.commit()
finally:
    conn.close()
PY

  stop_server

  start_server docker
  request_deploy_check_refresh
  visible_invalid_report="$(wait_for_compose_access_status pass)"
  reconciliation_state="$(python3 - "$DB_PATH" "$healthy_stack_id" <<'PY'
import json, sqlite3, sys

db_path, healthy_stack_id = sys.argv[1:3]
conn = sqlite3.connect(db_path)
try:
    rows = conn.execute(
        """
        SELECT d.project, d.status, d.archived, d.archived_reason,
               s.archived, s.archived_reason
        FROM discovered_compose_projects d
        JOIN stacks s ON s.id = d.stack_id
        WHERE d.project IN ('legacy-auto-stopped', 'missing-compose', 'invalid-auto', 'manual-stopped')
        ORDER BY d.project
        """
    ).fetchall()
    healthy = conn.execute(
        "SELECT archived FROM stacks WHERE id = ?", (healthy_stack_id,)
    ).fetchone()
finally:
    conn.close()

actual = {project: tuple(state) for project, *state in rows}
expected = {
    "invalid-auto": ("invalid", 0, None, 0, None),
    "legacy-auto-stopped": ("stopped", 0, None, 0, None),
    "manual-stopped": ("stopped", 1, "user_archive", 1, "user_archive"),
    "missing-compose": ("missing", 1, "auto_archive_compose_files_missing", 1, "auto_archive_compose_files_missing"),
}
if actual != expected:
    raise SystemExit(f"unexpected discovery reconciliation state: {actual}")
if healthy != (0,):
    raise SystemExit(f"active discovery Stack was unexpectedly archived: {healthy}")
print(json.dumps({
    "historicalAutoArchiveRestored": True,
    "missingComposeAutoArchived": True,
    "invalidComposeVisible": True,
    "manualArchiveProtected": True,
    "healthyStackArchived": False,
}, sort_keys=True))
PY
)"
  python3 - "$DB_PATH" <<'PY'
import sqlite3, sys

conn = sqlite3.connect(sys.argv[1])
try:
    conn.execute("UPDATE stacks SET archived = 1, archived_reason = 'user_archive' WHERE id = 'invalid-auto-stack'")
    conn.execute("UPDATE discovered_compose_projects SET archived = 1, archived_reason = 'user_archive' WHERE project = 'invalid-auto'")
    conn.commit()
finally:
    conn.close()
PY
  request_deploy_check_refresh
  manual_archive_report="$(wait_for_compose_access_status pass)"
  after_containers="$(compose_fixture ps -a -q)"
  after_running_containers="$(compose_fixture ps -q)"
  [[ "$before_containers" == "$after_containers" ]] || {
    echo "persisted discovery reconciliation changed fixture containers: before=$before_containers after=$after_containers" >&2
    return 1
  }
  [[ "$before_running_containers" == "$after_running_containers" ]] || {
    echo "persisted discovery reconciliation changed fixture running state: before=$before_running_containers after=$after_running_containers" >&2
    return 1
  }

  RESULTS+=("$(python3 - "$healthy_stack_id" "$stopped_compose_path" "$missing_compose_path" "$invalid_compose_path" "$manual_compose_path" "$healthy_report" "$visible_invalid_report" "$manual_archive_report" "$reconciliation_state" "$before_containers" "$after_containers" "$before_running_containers" "$after_running_containers" <<'PY'
import json, sys

healthy_stack_id, stopped_path, missing_path, invalid_path, manual_path, healthy, visible_invalid, manual_archive, state, before, after, before_running, after_running = sys.argv[1:]

def compose_access_status(raw: str) -> str:
    for item in json.loads(raw)["report"]["checks"]:
        if item["id"] == "core.compose_access":
            return item["status"]
    raise SystemExit("core.compose_access missing from report")

print(json.dumps({
    "mode": "persisted-discovery-reconciliation",
    "healthyStackId": healthy_stack_id,
    "stoppedComposePath": stopped_path,
    "missingComposePath": missing_path,
    "invalidComposePath": invalid_path,
    "manualComposePath": manual_path,
    "composeAccessWithVisibleInvalid": compose_access_status(visible_invalid),
    "composeAccessAfterInvalidManualArchive": compose_access_status(manual_archive),
    "healthyComposeAccess": compose_access_status(healthy),
    "reconciliation": json.loads(state),
    "containersBeforeRestart": before,
    "containersAfterRestart": after,
    "containerMutationObserved": before != after,
    "runningContainersBeforeRestart": before_running,
    "runningContainersAfterRestart": after_running,
    "containerStateMutationObserved": before_running != after_running,
}, sort_keys=True))
PY
)")
  stop_server
  compose_fixture down -v --remove-orphans >&2
}

run_v2_mode plugin docker
run_v2_mode standalone "$REMOTE_RUN/bin/docker-compose-v2"
run_v1_mode
run_persisted_discovery_reconciliation

summary="$(python3 - "$RUN_ID" "$REMOTE_RUN" "$FIXTURE_PROJECT" "${RESULTS[@]}" <<'PY'
import json, sys
run_id, remote_run, fixture_project = sys.argv[1:4]
results = [json.loads(value) for value in sys.argv[4:]]
print(json.dumps({
    "runId": run_id,
    "remoteRun": remote_run,
    "fixtureProject": fixture_project,
    "composeProjectsExplicit": True,
    "modes": results,
    "cleanupScope": remote_run,
    "keptForReview": False,
}, ensure_ascii=False, sort_keys=True))
PY
)"
if [[ "$KEEP_RUN" == "1" ]]; then
  summary="$(python3 - "$summary" <<'PY'
import json, sys
payload = json.loads(sys.argv[1])
payload["keptForReview"] = True
print(json.dumps(payload, ensure_ascii=False, sort_keys=True))
PY
)"
fi
printf '%s' "$summary" > artifacts/summary.json
printf '%s' "$summary"
REMOTE_SCRIPT

if [[ -n "$JSON_OUT" ]]; then
  mkdir -p "$(dirname "$JSON_OUT")"
  cp "$SUMMARY_TMP" "$JSON_OUT"
fi

printf '==> Validation summary\n'
python3 -m json.tool "$SUMMARY_TMP"
if [[ "$KEEP_RUN" == "1" ]]; then
  printf '\nRemote run kept for review: %s\n' "$REMOTE_RUN"
else
  printf '\nRemote run cleaned after validation: %s\n' "$REMOTE_RUN"
fi
