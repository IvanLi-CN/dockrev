#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

echo "[contract-check] syntax + workflow yaml parse"
bash -n .github/scripts/label-gate.sh .github/scripts/release-intent.sh .github/scripts/compute-version.sh
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/label-gate.yml"); YAML.load_file(".github/workflows/release.yml")'

has_rg() {
  command -v rg >/dev/null 2>&1
}

search_regex() {
  local pattern="$1"
  local file="$2"
  if has_rg; then
    rg -q -- "${pattern}" "${file}"
  else
    grep -Eq -- "${pattern}" "${file}"
  fi
}

count_fixed_lines() {
  local needle="$1"
  local file="$2"
  if has_rg; then
    rg -n -F -- "${needle}" "${file}" | wc -l | tr -d ' '
  else
    grep -n -F -- "${needle}" "${file}" | wc -l | tr -d ' '
  fi
}

echo "[contract-check] release workflow rc gating invariants"
search_regex "steps\\.intent\\.outputs\\.release_channel == 'rc'" .github/workflows/release.yml
search_regex "prerelease: \\$\\{\\{ env.RELEASE_CHANNEL == 'rc' \\}\\}" .github/workflows/release.yml
latest_gate_count="$(count_fixed_lines 'if [[ "${RELEASE_CHANNEL}" != "rc" ]]; then' .github/workflows/release.yml)"
if [[ "${latest_gate_count}" -lt 2 ]]; then
  echo "[contract-check] expected >=2 latest gates, got ${latest_gate_count}" >&2
  exit 1
fi

ensure_regex_absent() {
  local pattern="$1"
  local file="$2"
  if search_regex "${pattern}" "${file}"; then
    echo "[contract-check] unexpected pattern '${pattern}' in ${file}" >&2
    exit 1
  fi
}

echo "[contract-check] trusted label-gate workflow invariants"
search_regex "^[[:space:]]*pull_request_target:" .github/workflows/label-gate.yml
search_regex "^[[:space:]]*merge_group:" .github/workflows/label-gate.yml
search_regex "pull-requests:[[:space:]]*read" .github/workflows/label-gate.yml
search_regex "uses:[[:space:]]*actions/github-script@" .github/workflows/label-gate.yml
search_regex "resolveMergeGroupPullNumbers" .github/workflows/label-gate.yml
search_regex "GET /repos/\{owner\}/\{repo\}/commits/\{commit_sha\}/pulls" .github/workflows/label-gate.yml
ensure_regex_absent "^[[:space:]]*pull_request:" .github/workflows/label-gate.yml
ensure_regex_absent "run:[[:space:]]*bash[[:space:]]+\./\.github/scripts/label-gate\.sh" .github/workflows/label-gate.yml
tmp_dir="$(mktemp -d)"
server_pid=""
cleanup() {
  if [[ -n "${server_pid}" ]]; then
    kill "${server_pid}" >/dev/null 2>&1 || true
    wait "${server_pid}" 2>/dev/null || true
  fi
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

pick_free_port() {
  python3 - <<'PY'
import socket

sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
}

cat >"${tmp_dir}/mock_github_api.py" <<'PY'
#!/usr/bin/env python3
from __future__ import annotations

import json
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse

PULLS = {
    "sha_stable": [{"number": 101, "html_url": "https://example/pr/101"}],
    "sha_rc": [{"number": 102, "html_url": "https://example/pr/102"}],
    "sha_docs_stable": [{"number": 106, "html_url": "https://example/pr/106"}],
    "sha_missing": [{"number": 103, "html_url": "https://example/pr/103"}],
    "sha_multi": [{"number": 104, "html_url": "https://example/pr/104"}],
    "sha_unknown": [{"number": 105, "html_url": "https://example/pr/105"}],
}

LABELS = {
    101: [{"name": "type:patch"}, {"name": "channel:stable"}],
    102: [{"name": "type:patch"}, {"name": "channel:rc"}],
    103: [{"name": "type:patch"}],
    104: [{"name": "type:patch"}, {"name": "channel:stable"}, {"name": "channel:rc"}],
    105: [{"name": "type:patch"}, {"name": "channel:prerelease"}],
    106: [{"name": "type:docs"}, {"name": "channel:stable"}],
}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        return

    def _json(self, payload: object, status: int = 200):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = urlparse(self.path).path
        if path == "/health":
            return self._json({"ok": True})
        parts = [part for part in path.split("/") if part]
        if len(parts) >= 6 and parts[0] == "repos" and parts[3] == "commits" and parts[5] == "pulls":
            sha = parts[4]
            return self._json(PULLS.get(sha, []))
        if len(parts) >= 6 and parts[0] == "repos" and parts[3] == "issues" and parts[5] == "labels":
            try:
                pr_number = int(parts[4])
            except ValueError:
                return self._json([], status=400)
            return self._json(LABELS.get(pr_number, []))
        return self._json({"error": "not found", "path": path}, status=404)


if __name__ == "__main__":
    import os

    port = int(os.environ["MOCK_PORT"])
    server = HTTPServer(("127.0.0.1", port), Handler)
    server.serve_forever()
PY

wait_mock_server_ready() {
  for _ in $(seq 1 40); do
    if ! kill -0 "${server_pid}" >/dev/null 2>&1; then
      return 1
    fi
    if curl -sSf "http://127.0.0.1:${api_port}/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done
  return 1
}

start_mock_server() {
  local attempt
  for attempt in 1 2 3 4 5; do
    api_port="$(pick_free_port)"
    : >"${tmp_dir}/mock.log"
    MOCK_PORT="${api_port}" python3 "${tmp_dir}/mock_github_api.py" >"${tmp_dir}/mock.log" 2>&1 &
    server_pid="$!"
    if wait_mock_server_ready; then
      return 0
    fi
    kill "${server_pid}" >/dev/null 2>&1 || true
    wait "${server_pid}" 2>/dev/null || true
    server_pid=""
    if [[ "${attempt}" -lt 5 ]]; then
      sleep 0.1
    fi
  done
  return 1
}

if ! start_mock_server; then
  echo "[contract-check] mock api server did not become ready in time" >&2
  if [[ -s "${tmp_dir}/mock.log" ]]; then
    cat "${tmp_dir}/mock.log" >&2 || true
  fi
  exit 1
fi

run_label_gate() {
  local pr_number="$1"
  local expected_status="$2"
  set +e
  local output
  output="$(
    GITHUB_API_URL="http://127.0.0.1:${api_port}" \
      GITHUB_REPOSITORY="IvanLi-CN/dockrev" \
      GITHUB_TOKEN="x" \
      PR_NUMBER="${pr_number}" \
      bash ./.github/scripts/label-gate.sh 2>&1
  )"
  local code=$?
  set -e
  if [[ "${expected_status}" == "ok" ]]; then
    if [[ "${code}" -ne 0 ]]; then
      echo "[contract-check] label-gate pr=${pr_number} expected ok, got ${code}" >&2
      echo "${output}" >&2
      exit 1
    fi
  else
    if [[ "${code}" -eq 0 ]]; then
      echo "[contract-check] label-gate pr=${pr_number} expected fail, got success" >&2
      echo "${output}" >&2
      exit 1
    fi
  fi
}

run_release_intent() {
  local sha="$1"
  local expected_should_release="$2"
  local expected_channel="$3"
  local expected_reason_prefix="$4"
  local out_file="${tmp_dir}/release-intent-${sha}.out"
  GITHUB_OUTPUT="${out_file}" \
    GITHUB_API_URL="http://127.0.0.1:${api_port}" \
    GITHUB_REPOSITORY="IvanLi-CN/dockrev" \
    GITHUB_TOKEN="x" \
    WORKFLOW_RUN_SHA="${sha}" \
    bash ./.github/scripts/release-intent.sh >/dev/null
  local should_release
  local release_channel
  local reason
  should_release="$(sed -n 's/^should_release=//p' "${out_file}" | tail -n1)"
  release_channel="$(sed -n 's/^release_channel=//p' "${out_file}" | tail -n1)"
  reason="$(sed -n 's/^reason=//p' "${out_file}" | tail -n1)"
  if [[ "${should_release}" != "${expected_should_release}" ]]; then
    echo "[contract-check] release-intent ${sha}: should_release=${should_release} expected=${expected_should_release}" >&2
    exit 1
  fi
  if [[ "${release_channel}" != "${expected_channel}" ]]; then
    echo "[contract-check] release-intent ${sha}: release_channel=${release_channel} expected=${expected_channel}" >&2
    exit 1
  fi
  if [[ "${reason}" != "${expected_reason_prefix}"* ]]; then
    echo "[contract-check] release-intent ${sha}: reason=${reason} expected_prefix=${expected_reason_prefix}" >&2
    exit 1
  fi
}

echo "[contract-check] label-gate scenarios"
run_label_gate 101 ok
run_label_gate 102 ok
run_label_gate 106 ok
run_label_gate 103 fail
run_label_gate 104 fail
run_label_gate 105 fail

echo "[contract-check] release-intent scenarios"
run_release_intent sha_stable true stable intent_release
run_release_intent sha_rc true rc intent_release
run_release_intent sha_docs_stable false stable intent_skip
run_release_intent sha_missing false stable invalid_channel_label_count
run_release_intent sha_multi false stable invalid_channel_label_count
run_release_intent sha_unknown false stable unknown_channel_label

echo "[contract-check] all checks passed"
