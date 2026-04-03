#!/usr/bin/env bash
set -euo pipefail

require_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "[deploy] missing required env: ${name}" >&2
    exit 1
  fi
}

require_env RELEASE_TAG
require_env PROD_DEPLOY_HOST
require_env PROD_DEPLOY_USER
require_env PROD_DEPLOY_STACK_DIR
require_env PROD_DEPLOY_COMPOSE_FILE
require_env PROD_DEPLOY_VERSION_URL
require_env PROD_DEPLOY_SSH_KEY
require_env PROD_DEPLOY_SSH_KNOWN_HOSTS

port="${PROD_DEPLOY_PORT:-22}"
services_raw="${PROD_DEPLOY_SERVICES:-dockrev supervisor}"
version_timeout_seconds="${PROD_DEPLOY_VERSION_TIMEOUT_SECONDS:-90}"
read -r -a services <<<"${services_raw}"
if (( ${#services[@]} == 0 )); then
  echo "[deploy] PROD_DEPLOY_SERVICES resolved to an empty service list" >&2
  exit 1
fi

key_file="$(mktemp)"
known_hosts_file="$(mktemp)"
cleanup() {
  rm -f "${key_file}" "${known_hosts_file}"
}
trap cleanup EXIT

chmod 600 "${key_file}" "${known_hosts_file}"
printf '%s\n' "${PROD_DEPLOY_SSH_KEY}" >"${key_file}"
printf '%s\n' "${PROD_DEPLOY_SSH_KNOWN_HOSTS}" >"${known_hosts_file}"

ssh_opts=(
  -i "${key_file}"
  -p "${port}"
  -o BatchMode=yes
  -o StrictHostKeyChecking=yes
  -o UserKnownHostsFile="${known_hosts_file}"
)

echo "[deploy] applying ${RELEASE_TAG} to ${PROD_DEPLOY_USER}@${PROD_DEPLOY_HOST}:${PROD_DEPLOY_STACK_DIR}"
ssh "${ssh_opts[@]}" "${PROD_DEPLOY_USER}@${PROD_DEPLOY_HOST}" \
  bash -s -- \
  "${PROD_DEPLOY_STACK_DIR}" \
  "${PROD_DEPLOY_COMPOSE_FILE}" \
  "${RELEASE_TAG}" \
  "${services[@]}" <<'REMOTE'
set -euo pipefail

stack_dir="$1"
compose_file="$2"
release_tag="$3"
shift 3
services=("$@")

cd "${stack_dir}"

docker compose -f "${compose_file}" pull "${services[@]}"
docker compose -f "${compose_file}" up -d "${services[@]}"
docker compose -f "${compose_file}" ps "${services[@]}"

for service in "${services[@]}"; do
  mapfile -t container_ids < <(docker compose -f "${compose_file}" ps -q "${service}")
  if (( ${#container_ids[@]} == 0 )); then
    echo "[deploy] service ${service} has no running container after deploy" >&2
    exit 1
  fi

  for container_id in "${container_ids[@]}"; do
    actual_version="$(docker inspect "${container_id}" --format '{{index .Config.Labels "org.opencontainers.image.version"}}')"
    if [[ "${actual_version}" != "${release_tag}" ]]; then
      echo "[deploy] service ${service} container ${container_id} version mismatch: got=${actual_version} expected=${release_tag}" >&2
      exit 1
    fi
  done
done
REMOTE

deadline="$((SECONDS + version_timeout_seconds))"
actual_version=""
while true; do
  request_timeout_seconds=5
  remaining_seconds="$((deadline - SECONDS))"
  if (( remaining_seconds < request_timeout_seconds )); then
    request_timeout_seconds="${remaining_seconds}"
  fi
  if (( request_timeout_seconds <= 0 )); then
    echo "[deploy] public version mismatch after ${version_timeout_seconds}s: got=${actual_version:-<unavailable>} expected=${RELEASE_TAG}" >&2
    exit 1
  fi

  if version_json="$(curl -fsSL --connect-timeout "${request_timeout_seconds}" --max-time "${request_timeout_seconds}" "${PROD_DEPLOY_VERSION_URL}" 2>/dev/null)"; then
    if actual_version="$(
      printf '%s' "${version_json}" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("version",""))' 2>/dev/null
    )" && [[ "${actual_version}" == "${RELEASE_TAG}" ]]; then
      break
    fi
  fi

  if (( SECONDS >= deadline )); then
    echo "[deploy] public version mismatch after ${version_timeout_seconds}s: got=${actual_version:-<unavailable>} expected=${RELEASE_TAG}" >&2
    exit 1
  fi

  sleep 2
done

echo "[deploy] public version ok: ${actual_version}"
