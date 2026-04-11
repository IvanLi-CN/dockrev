#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/verify_shared_testbox_cleanup.sh [--keep-run] [--run-id RUN_ID] [--testbox HOST] [--json-out PATH]

Deploy Dockrev plus an isolated fixture stack to the shared testbox, create stack-owned cleanup targets,
trigger discovery, run stack-scoped cleanup, and verify the targets were actually deleted.

Options:
  --keep-run        Leave the remote run directory and compose projects up for manual review.
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

require_cmd git
require_cmd ssh
require_cmd rsync
require_cmd python3
require_cmd curl

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

sanitize_slug() {
  python3 - "$1" <<'PY'
import re, sys
s=sys.argv[1].lower()
s=re.sub(r'[^a-z0-9_-]+','_',s).strip('_')
print(s[:63] if len(s) > 63 else s)
PY
}

FIXTURE_PROJECT="$(sanitize_slug "fx_${REPO_NAME}_${RUN_ID}")"
DEPLOY_PROJECT="$(sanitize_slug "dockrev_${REPO_NAME}_${RUN_ID}")"
FIXTURE_IMAGE_REPO="ghcr.io/dockrev-fixtures/${FIXTURE_PROJECT}/app"
DOCKREV_IMAGE="dockrev:testbox-${RUN_ID}"
SUPERVISOR_IMAGE="dockrev-supervisor:testbox-${RUN_ID}"
CREATED_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

REMOTE_GATEWAY_PORT="$(ssh -o BatchMode=yes "$TESTBOX" "python3 - <<'PY'
import socket
sock = socket.socket()
sock.bind(('127.0.0.1', 0))
print(sock.getsockname()[1])
sock.close()
PY")"

SUMMARY_TMP="$(mktemp -t dockrev-testbox-cleanup-summary.XXXXXX.json)"
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
ssh -o BatchMode=yes "$TESTBOX" "mkdir -p '$REMOTE_RUN' '$REMOTE_WORKSPACE' && cat > '$REMOTE_WORKSPACE/workspace.txt'" <<TXT
local_repo_root=$REPO_ROOT
created_utc=$CREATED_UTC
last_run_id=$RUN_ID
TXT

printf '==> Syncing repo to shared testbox\n'
rsync -az --delete \
  "${SYNC_EXCLUDES[@]}" \
  "$REPO_ROOT/" "$TESTBOX:$REMOTE_RUN/"

printf '==> Running shared-testbox cleanup validation\n'
ssh -o BatchMode=yes "$TESTBOX" \
  env \
    REMOTE_RUN="$REMOTE_RUN" \
    RUN_ID="$RUN_ID" \
    FIXTURE_PROJECT="$FIXTURE_PROJECT" \
    DEPLOY_PROJECT="$DEPLOY_PROJECT" \
    FIXTURE_IMAGE_REPO="$FIXTURE_IMAGE_REPO" \
    DOCKREV_IMAGE="$DOCKREV_IMAGE" \
    SUPERVISOR_IMAGE="$SUPERVISOR_IMAGE" \
    REMOTE_GATEWAY_PORT="$REMOTE_GATEWAY_PORT" \
    KEEP_RUN="$KEEP_RUN" \
  'bash -s' > "$SUMMARY_TMP" <<'REMOTE_SCRIPT'
set -euo pipefail

cd "$REMOTE_RUN"
mkdir -p fixture deploy/data deploy/data/supervisor artifacts

log() {
  printf ':: %s\n' "$*" >&2
}

json_field() {
  local path="$1"
  local json_input="$2"
  python3 - "$path" "$json_input" <<'PY'
import json, sys
path = [p for p in sys.argv[1].split('.') if p]
cur = json.loads(sys.argv[2])
for part in path:
    if part.isdigit():
        cur = cur[int(part)]
    else:
        cur = cur[part]
if isinstance(cur, (dict, list)):
    print(json.dumps(cur))
else:
    print(cur)
PY
}

curl_json() {
  local method="$1"
  local path="$2"
  local body="${3:-}"
  local url="http://127.0.0.1:${REMOTE_GATEWAY_PORT}${path}"
  if [[ -n "$body" ]]; then
    curl --silent --show-error --fail-with-body \
      -H 'content-type: application/json' \
      -X "$method" \
      "$url" \
      --data "$body"
  else
    curl --silent --show-error --fail-with-body \
      -H 'content-type: application/json' \
      -X "$method" \
      "$url"
  fi
}

pick_compose_services() {
  local compose_file="$1"
  local override_file="${2:-}"
  if [[ -n "$override_file" ]]; then
    DOCKREV_GATEWAY_BIND="127.0.0.1:${REMOTE_GATEWAY_PORT}:80" docker compose -f "$compose_file" -f "$override_file" config --services
  else
    docker compose -f "$compose_file" config --services
  fi
}

generate_caps_override() {
  local compose_file="$1"
  local override_file="${2:-}"
  local out_file="$3"
  local services
  services="$(pick_compose_services "$compose_file" "$override_file")"
  {
    echo 'services:'
    while IFS= read -r svc; do
      [[ -n "$svc" ]] || continue
      cat <<YAML
  $svc:
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
  } > "$out_file"
}

wait_http_ok() {
  local url="$1"
  local attempts="${2:-60}"
  for ((i=1; i<=attempts; i++)); do
    if curl --silent --show-error --fail "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done
  echo "timed out waiting for $url" >&2
  return 1
}

poll_job_terminal() {
  local job_id="$1"
  local status=""
  local body=""
  for ((i=1; i<=90; i++)); do
    body="$(curl_json GET "/api/jobs/${job_id}")"
    status="$(json_field job.status "$body")"
    if [[ "$status" != "running" && "$status" != "queued" ]]; then
      printf '%s' "$body"
      return 0
    fi
    sleep 2
  done
  echo "timed out waiting for job ${job_id}" >&2
  return 1
}

cleanup_remote() {
  set +e
  if [[ -f "$REMOTE_RUN/fixture/compose.yaml" && -f "$REMOTE_RUN/fixture/.codex.caps-compat.yaml" ]]; then
    docker compose -p "$FIXTURE_PROJECT" -f "$REMOTE_RUN/fixture/compose.yaml" -f "$REMOTE_RUN/fixture/.codex.caps-compat.yaml" down -v --remove-orphans >/dev/null 2>&1 || true
  fi
  if [[ -f "$REMOTE_RUN/deploy/docker-compose.yml" && -f "$REMOTE_RUN/deploy/.codex.override.yaml" && -f "$REMOTE_RUN/deploy/.codex.caps-compat.yaml" ]]; then
    DOCKREV_GATEWAY_BIND="127.0.0.1:${REMOTE_GATEWAY_PORT}:80" \
      docker compose -p "$DEPLOY_PROJECT" \
      -f "$REMOTE_RUN/deploy/docker-compose.yml" \
      -f "$REMOTE_RUN/deploy/.codex.override.yaml" \
      -f "$REMOTE_RUN/deploy/.codex.caps-compat.yaml" \
      down -v --remove-orphans >/dev/null 2>&1 || true
  fi
  docker rm -f "${FIXTURE_PROJECT}-ghost" >/dev/null 2>&1 || true
  docker image rm "$FIXTURE_IMAGE_REPO:live" "$FIXTURE_IMAGE_REPO:old" "$DOCKREV_IMAGE" "$SUPERVISOR_IMAGE" >/dev/null 2>&1 || true
  if [[ "$KEEP_RUN" != "1" ]]; then
    rm -rf "$REMOTE_RUN"
  fi
}

trap 'if [[ "$KEEP_RUN" != "1" ]]; then cleanup_remote; fi' EXIT

cat > "$REMOTE_RUN/fixture/Dockerfile" <<'EOF_DOCKERFILE'
FROM alpine:3.20
ARG MARKER=unset
RUN printf '%s\n' "$MARKER" > /marker.txt
CMD ["sh", "-lc", "sleep infinity"]
EOF_DOCKERFILE

cat > "$REMOTE_RUN/fixture/compose.yaml" <<EOF_FIXTURE
services:
  app:
    image: ${FIXTURE_IMAGE_REPO}:live
    build:
      context: .
      dockerfile: Dockerfile
      args:
        MARKER: live
    command: ["sh", "-lc", "sleep infinity"]
    restart: unless-stopped
EOF_FIXTURE

cat > "$REMOTE_RUN/deploy/.codex.override.yaml" <<EOF_DEPLOY
services:
  dockrev:
    image: ${DOCKREV_IMAGE}
    environment:
      DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV: "true"
    volumes:
      - /srv/codex:/srv/codex:ro
  supervisor:
    image: ${SUPERVISOR_IMAGE}
    environment:
      DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV: "true"
    volumes:
      - /srv/codex:/srv/codex:ro
EOF_DEPLOY

generate_caps_override "$REMOTE_RUN/fixture/compose.yaml" "" "$REMOTE_RUN/fixture/.codex.caps-compat.yaml"
generate_caps_override "$REMOTE_RUN/deploy/docker-compose.yml" "$REMOTE_RUN/deploy/.codex.override.yaml" "$REMOTE_RUN/deploy/.codex.caps-compat.yaml"

log "building fixture images"
docker build -t "${FIXTURE_IMAGE_REPO}:old" --build-arg MARKER=old "$REMOTE_RUN/fixture" >&2

log "starting fixture compose project ${FIXTURE_PROJECT}"
docker compose -p "$FIXTURE_PROJECT" -f "$REMOTE_RUN/fixture/compose.yaml" -f "$REMOTE_RUN/fixture/.codex.caps-compat.yaml" up -d --build >&2

docker create \
  --name "${FIXTURE_PROJECT}-ghost" \
  --label "com.docker.compose.project=${FIXTURE_PROJECT}" \
  --label "com.docker.compose.service=app" \
  "${FIXTURE_IMAGE_REPO}:live" \
  sh -lc 'exit 0' >/dev/null

log "starting dockrev deploy project ${DEPLOY_PROJECT} on 127.0.0.1:${REMOTE_GATEWAY_PORT}"
DOCKREV_GATEWAY_BIND="127.0.0.1:${REMOTE_GATEWAY_PORT}:80" \
  docker compose -p "$DEPLOY_PROJECT" \
  -f "$REMOTE_RUN/deploy/docker-compose.yml" \
  -f "$REMOTE_RUN/deploy/.codex.override.yaml" \
  -f "$REMOTE_RUN/deploy/.codex.caps-compat.yaml" \
  up -d --build >&2

wait_http_ok "http://127.0.0.1:${REMOTE_GATEWAY_PORT}/api/health"

log "triggering discovery scan"
discovery_resp="$(curl_json POST /api/discovery/scan '{}')"
discovery_job_id="$(json_field jobId "$discovery_resp")"
discovery_job="$(poll_job_terminal "$discovery_job_id")"
discovery_status="$(json_field job.status "$discovery_job")"
if [[ "$discovery_status" != "success" ]]; then
  echo "discovery failed: $discovery_job" >&2
  exit 1
fi

projects_resp="$(curl_json GET /api/discovery/projects)"
stack_id="$(python3 - "$FIXTURE_PROJECT" "$projects_resp" <<'PY'
import json, sys
project = sys.argv[1]
payload = json.loads(sys.argv[2])
for item in payload["projects"]:
    if item.get("project") == project:
        stack_id = item.get("stackId")
        if not stack_id:
            raise SystemExit(f"project {project} has no stackId yet")
        print(stack_id)
        break
else:
    raise SystemExit(f"project {project} not found")
PY
)"

scan_payload="$(python3 - "$stack_id" <<'PY'
import json, sys
print(json.dumps({
    "reason": "confirm",
    "preset": "balanced",
    "scope": "stack",
    "stackId": sys.argv[1],
}))
PY
)"
log "requesting cleanup confirm-scan for stack ${stack_id}"
scan_resp="$(curl_json POST /api/cleanups/scan "$scan_payload")"
fingerprint="$(json_field confirmationFingerprint "$scan_resp")"

python3 - "$FIXTURE_IMAGE_REPO:old" "${FIXTURE_PROJECT}-ghost" "$scan_resp" <<'PY'
import json, sys
scan = json.loads(sys.argv[3])
stack_groups = scan.get("stackGroups") or []
resources = []
for group in stack_groups:
    resources.extend(group.get("stackOrphans") or [])
    for svc in group.get("services") or []:
        resources.extend(svc.get("resources") or [])
labels = {item.get("label") for item in resources}
missing = [label for label in sys.argv[1:3] if label not in labels]
if missing:
    raise SystemExit(f"expected cleanup targets missing from confirm-scan: {missing}; labels={sorted(labels)}")
PY

apply_payload="$(python3 - "$stack_id" "$fingerprint" <<'PY'
import json, sys
print(json.dumps({
    "reason": "ui",
    "preset": "balanced",
    "scope": "stack",
    "stackId": sys.argv[1],
    "confirmationFingerprint": sys.argv[2],
}))
PY
)"
log "applying cleanup for stack ${stack_id}"
apply_resp="$(curl_json POST /api/cleanups/apply "$apply_payload")"
cleanup_job_id="$(json_field jobId "$apply_resp")"
cleanup_job="$(poll_job_terminal "$cleanup_job_id")"
cleanup_status="$(json_field job.status "$cleanup_job")"
if [[ "$cleanup_status" != "success" ]]; then
  echo "cleanup job failed: $cleanup_job" >&2
  exit 1
fi

if docker container inspect "${FIXTURE_PROJECT}-ghost" >/dev/null 2>&1; then
  echo "expected ghost container to be deleted" >&2
  exit 1
fi
if docker image inspect "${FIXTURE_IMAGE_REPO}:old" >/dev/null 2>&1; then
  echo "expected old image to be deleted" >&2
  exit 1
fi
if ! docker compose -p "$FIXTURE_PROJECT" -f "$REMOTE_RUN/fixture/compose.yaml" -f "$REMOTE_RUN/fixture/.codex.caps-compat.yaml" ps --status running | grep -q 'app'; then
  echo "expected live fixture service to still be running" >&2
  exit 1
fi

post_scan_resp="$(curl_json POST /api/cleanups/scan "$scan_payload")"
python3 - "$FIXTURE_IMAGE_REPO:old" "${FIXTURE_PROJECT}-ghost" "$post_scan_resp" <<'PY'
import json, sys
scan = json.loads(sys.argv[3])
stack_groups = scan.get("stackGroups") or []
resources = []
for group in stack_groups:
    resources.extend(group.get("stackOrphans") or [])
    for svc in group.get("services") or []:
        resources.extend(svc.get("resources") or [])
labels = {item.get("label") for item in resources}
remaining = [label for label in sys.argv[1:3] if label in labels]
if remaining:
    raise SystemExit(f"cleanup targets still present after apply: {remaining}")
PY

summary_json="$(python3 - "$RUN_ID" "$REMOTE_RUN" "$FIXTURE_PROJECT" "$DEPLOY_PROJECT" "$stack_id" "$cleanup_job_id" "$REMOTE_GATEWAY_PORT" "$cleanup_job" <<'PY'
import json, sys
job = json.loads(sys.argv[8])["job"]
summary = job.get("summary", {})
out = {
    "runId": sys.argv[1],
    "remoteRun": sys.argv[2],
    "fixtureProject": sys.argv[3],
    "deployProject": sys.argv[4],
    "stackId": sys.argv[5],
    "cleanupJobId": sys.argv[6],
    "remoteGatewayPort": int(sys.argv[7]),
    "cleanupStatus": job.get("status"),
    "deletedCountsByKind": summary.get("deletedCountsByKind", {}),
    "groupedTargets": summary.get("groupedTargets", []),
    "reclaimedBytesEstimated": summary.get("reclaimedBytesEstimated"),
    "keptForReview": False,
}
print(json.dumps(out, ensure_ascii=False, sort_keys=True))
PY
)"

if [[ "$KEEP_RUN" == "1" ]]; then
  summary_json="$(python3 - "$summary_json" <<'PY'
import json, sys
obj = json.loads(sys.argv[1])
obj["keptForReview"] = True
print(json.dumps(obj, ensure_ascii=False, sort_keys=True))
PY
)"
fi

printf '%s\n' "$summary_json" > "$REMOTE_RUN/artifacts/validation-summary.json"
printf '%s' "$summary_json"
REMOTE_SCRIPT

if [[ -n "$JSON_OUT" ]]; then
  mkdir -p "$(dirname "$JSON_OUT")"
  cp "$SUMMARY_TMP" "$JSON_OUT"
fi

printf '==> Validation summary\n'
python3 -m json.tool "$SUMMARY_TMP"

if [[ "$KEEP_RUN" == "1" ]]; then
  printf '\nRemote review URL: http://127.0.0.1:%s/\n' "$REMOTE_GATEWAY_PORT"
  printf 'Remote run path: %s\n' "$REMOTE_RUN"
else
  printf '\nRemote run cleaned after validation: %s\n' "$REMOTE_RUN"
fi
