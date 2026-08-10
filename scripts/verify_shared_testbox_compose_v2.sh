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

for command in git ssh rsync python3; do
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
PATH_HASH8="$(python3 - "$REPO_ROOT" <<'PY'
import hashlib, os, sys
print(hashlib.sha256(os.path.realpath(sys.argv[1]).encode()).hexdigest()[:8])
PY
)"
GIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo nogit)"
if [[ -z "$RUN_ID" ]]; then
  RUN_ID="$(date -u +%Y%m%d_%H%M%S)_${GIT_SHA}"
fi
WORKSPACE_SLUG="${REPO_NAME}__${PATH_HASH8}"
REMOTE_BASE="/srv/codex/workspaces/$USER"
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
suffix = f"{run_slug}_{entropy}"
max_repo_len = max(1, 63 - len(prefix_slug) - len(suffix) - 2)
print(f"{prefix_slug}_{repo_slug[:max_repo_len]}_{suffix}")
PY
}

FIXTURE_PROJECT="$(compose_project_slug "composev2" "$REPO_NAME" "$RUN_ID")"
SUMMARY_TMP="$(mktemp -t dockrev-testbox-compose-v2-summary.XXXXXX.json)"
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
ssh -o BatchMode=yes "$TESTBOX" "mkdir -p '$REMOTE_RUN' '$REMOTE_WORKSPACE' && printf '%s\\n' 'local_repo_root=$REPO_ROOT' 'created_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)' 'last_run_id=$RUN_ID' > '$REMOTE_WORKSPACE/workspace.txt'"

printf '==> Syncing repository to shared testbox\n'
rsync -az --delete "${SYNC_EXCLUDES[@]}" "$REPO_ROOT/" "$TESTBOX:$REMOTE_RUN/"

printf '==> Running real Compose V2 regression on shared testbox\n'
ssh -o BatchMode=yes "$TESTBOX" \
  env \
    REMOTE_RUN="$REMOTE_RUN" \
    RUN_ID="$RUN_ID" \
    FIXTURE_PROJECT="$FIXTURE_PROJECT" \
    KEEP_RUN="$KEEP_RUN" \
  'bash -s' > "$SUMMARY_TMP" <<'REMOTE_SCRIPT'
set -euo pipefail

cd "$REMOTE_RUN"
mkdir -p fixture bin artifacts

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
    if [[ "$(json_get status "$payload")" == "ready" && "$(json_get refreshing "$payload")" == "False" ]]; then
      printf '%s' "$payload"
      return 0
    fi
    sleep 1
  done
  echo "timed out waiting for deploy-check report" >&2
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
  compose_fixture down -v --remove-orphans >/dev/null 2>&1 || true
  if [[ "$KEEP_RUN" != "1" ]]; then
    rm -rf "$REMOTE_RUN"
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

run_v2_mode plugin docker
run_v2_mode standalone "$REMOTE_RUN/bin/docker-compose-v2"
run_v1_mode

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
