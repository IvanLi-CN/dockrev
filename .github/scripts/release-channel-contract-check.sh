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

search_fixed() {
  local needle="$1"
  local file="$2"
  if has_rg; then
    rg -q -F -- "${needle}" "${file}"
  else
    grep -Fq -- "${needle}" "${file}"
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

ensure_regex_absent() {
  local pattern="$1"
  local file="$2"
  if search_regex "${pattern}" "${file}"; then
    echo "[contract-check] unexpected pattern '${pattern}' in ${file}" >&2
    exit 1
  fi
}

echo "[contract-check] release workflow rc gating invariants"
search_regex "steps\\.intent\\.outputs\\.release_channel == 'rc'" .github/workflows/release.yml
python3 - <<'PY'
from pathlib import Path
text = Path('.github/workflows/release.yml').read_text()
needle = "prerelease: ${{ env.RELEASE_CHANNEL == 'rc' }}"
if needle not in text:
    raise SystemExit('[contract-check] expected prerelease gate in release workflow')
PY
latest_gate_count="$(count_fixed_lines 'if [[ "${RELEASE_CHANNEL}" != "rc" ]]; then' .github/workflows/release.yml)"
if [[ "${latest_gate_count}" -lt 2 ]]; then
  echo "[contract-check] expected >=2 latest gates, got ${latest_gate_count}" >&2
  exit 1
fi

echo "[contract-check] quality-gate workflow invariants"
search_regex "^[[:space:]]*pull_request_target:" .github/workflows/label-gate.yml
search_regex "^[[:space:]]*merge_group:" .github/workflows/label-gate.yml
search_regex "pull-requests:[[:space:]]*read" .github/workflows/label-gate.yml
search_regex "uses:[[:space:]]*actions/github-script@" .github/workflows/label-gate.yml
search_regex "resolveMergeGroupPullNumbers" .github/workflows/label-gate.yml
search_regex "GET /repos/\\{owner\\}/\\{repo\\}/commits/\\{commit_sha\\}/pulls" .github/workflows/label-gate.yml
search_regex "context\\.eventName === 'merge_group'" .github/workflows/label-gate.yml
search_regex "context\\.payload\\.pull_request\\?\\.number" .github/workflows/label-gate.yml
ensure_regex_absent "^[[:space:]]*pull_request:" .github/workflows/label-gate.yml
ensure_regex_absent "run:[[:space:]]*bash[[:space:]]+\\./\\.github/scripts/label-gate\\.sh" .github/workflows/label-gate.yml
ensure_regex_absent "context\\.eventName === 'pull_request'" .github/workflows/label-gate.yml
ensure_regex_absent "head_commit\\?\\.message" .github/workflows/label-gate.yml

if [[ -e .github/workflows/review-policy.yml ]]; then
  echo "[contract-check] review-policy workflow must be removed; review policy is GitHub-native now" >&2
  exit 1
fi

python3 - <<'PY'
from __future__ import annotations
import json
from pathlib import Path

path = Path('.github/quality-gates.json')
data = json.loads(path.read_text())
policy = data['policy']
review = policy['review_policy']
enforcement = review['enforcement']
required_checks = data.get('required_checks', [])
informational_checks = data.get('informational_checks', [])
expected_pr_workflows = data.get('expected_pr_workflows', [])

assert policy['require_signed_commits'] is True, 'require_signed_commits must stay true'
assert policy['branch_protection']['require_pull_request'] is True, 'default branch must stay PR-only'
assert policy['branch_protection']['disallow_direct_pushes'] is True, 'default branch must disallow direct pushes'
assert review['mode'] == 'conditional-required', 'review_policy.mode must stay conditional-required'
assert review['required_approvals'] == 1, 'review_policy.required_approvals must stay 1'
assert review['exempt_repository_owner'] is True, 'repository owner must stay exempt'
assert review['exempt_author_permissions'] == ['admin', 'maintain'], 'exempt author permissions drifted'
assert review['allowed_reviewer_permissions'] == ['write', 'maintain', 'admin'], 'allowed reviewer permissions drifted'
assert enforcement['mode'] == 'github-native', 'review policy enforcement must be github-native'
assert enforcement['bypass_mode'] == 'pull-request-only', 'review policy bypass must stay PR-only'
assert 'Review Policy Gate' not in required_checks, 'legacy Review Policy Gate must not stay required'
assert 'Review Policy Gate' not in informational_checks, 'legacy Review Policy Gate must not stay informational'
assert all(item.get('workflow') != 'Review Policy' for item in expected_pr_workflows), 'Review Policy workflow must not stay declared'
assert 'Release intent label gate' in required_checks, 'label gate must stay required'
PY

python3 ./.github/scripts/check-live-quality-gates.py --declaration .github/quality-gates.json || exit 1

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

REQUIRED_CHECKS = [
    "Release intent label gate",
    "Detect changes",
    "Lint & Checks",
    "Backend Tests",
    "Frontend lint + build",
    "Frontend Storybook build",
    "Frontend Storybook test",
    "Release build check (PR)",
]

RULES_BY_BRANCH = {
    "main": {
        "rules": [
            {
                "type": "pull_request",
                "parameters": {
                    "required_approving_review_count": 1,
                    "dismiss_stale_reviews_on_push": False,
                    "require_code_owner_review": False,
                    "require_last_push_approval": False,
                    "required_review_thread_resolution": False,
                    "allowed_merge_methods": ["merge", "squash", "rebase"],
                },
            },
            {
                "type": "required_status_checks",
                "parameters": {
                    "strict_required_status_checks_policy": True,
                    "required_status_checks": [
                        {"context": context, "integration_id": 15368}
                        for context in REQUIRED_CHECKS
                    ],
                },
            },
            {"type": "required_signatures"},
            {"type": "non_fast_forward"},
        ]
    },
    "main-missing-review": {
        "rules": [
            {
                "type": "pull_request",
                "parameters": {
                    "required_approving_review_count": 0,
                    "allowed_merge_methods": ["merge", "squash", "rebase"],
                },
            },
            {
                "type": "required_status_checks",
                "parameters": {
                    "strict_required_status_checks_policy": True,
                    "required_status_checks": [
                        {"context": context, "integration_id": 15368}
                        for context in REQUIRED_CHECKS
                    ],
                },
            },
            {"type": "required_signatures"},
        ]
    },
    "main-stale-check": {
        "rules": [
            {
                "type": "pull_request",
                "parameters": {
                    "required_approving_review_count": 1,
                    "allowed_merge_methods": ["merge", "squash", "rebase"],
                },
            },
            {
                "type": "required_status_checks",
                "parameters": {
                    "strict_required_status_checks_policy": True,
                    "required_status_checks": [
                        {"context": context, "integration_id": 15368}
                        for context in REQUIRED_CHECKS + ["Review Policy Gate"]
                    ],
                },
            },
            {"type": "required_signatures"},
            {"type": "non_fast_forward"},
        ]
    },
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
        if len(parts) >= 6 and parts[0] == "repos" and parts[3] == "rules" and parts[4] == "branches":
            branch = parts[5]
            payload = RULES_BY_BRANCH.get(branch)
            if payload is None:
                return self._json({"error": "branch not found", "branch": branch}, status=404)
            return self._json(payload)
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

extract_github_script() {
  local workflow_path="$1"
  local job_key="$2"
  local step_name="$3"
  local output_path="$4"
  ruby -ryaml -e '
    workflow = YAML.load_file(ARGV[0])
    job = workflow.fetch("jobs").fetch(ARGV[1])
    step = job.fetch("steps").find { |entry| entry["name"] == ARGV[2] } or abort("missing step")
    script = step.dig("with", "script") or abort("missing github-script body")
    File.write(ARGV[3], script)
  ' "${workflow_path}" "${job_key}" "${step_name}" "${output_path}"
}

run_inline_workflow_contract_checks() {
  local label_script="${tmp_dir}/label-gate.inline.js"
  extract_github_script \
    .github/workflows/label-gate.yml \
    label-gate \
    "Validate release intent + channel labels" \
    "${label_script}"

  node - "${label_script}" <<'NODE'
const fs = require('fs')

const labelScript = fs.readFileSync(process.argv[2], 'utf8')

function createCore() {
  return {
    failed: null,
    notices: [],
    summary: {
      addHeading() { return this },
      addRaw() { return this },
      addEOL() { return this },
      async write() { return this },
    },
    setFailed(message) { this.failed = String(message) },
    notice(message) { this.notices.push(String(message)) },
    getInput() { return '' },
  }
}

async function runGithubScript(script, { context, github, env = {} }) {
  const core = createCore()
  const previous = new Map()
  for (const [key, value] of Object.entries(env)) {
    previous.set(key, process.env[key])
    process.env[key] = value
  }
  try {
    const fn = new Function('context', 'github', 'core', `"use strict"; return (async () => {
${script}
})();`)
    await fn(context, github, core)
  } catch (error) {
    if (core.failed === null) {
      core.failed = error instanceof Error ? error.message : String(error)
    }
  } finally {
    for (const [key, value] of previous.entries()) {
      if (value === undefined) {
        delete process.env[key]
      } else {
        process.env[key] = value
      }
    }
  }
  return core
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message)
  }
}

function makeLabelGithub({ labelsByPull, commitPullsBySha }) {
  return {
    paginate: async (route, params) => {
      if (typeof route === 'string' && route.includes('/commits/{commit_sha}/pulls')) {
        return commitPullsBySha[params.commit_sha] || []
      }
      throw new Error(`unexpected label-gate paginate route: ${String(route)}`)
    },
    rest: {
      pulls: {
        get: async ({ pull_number }) => ({
          data: {
            number: pull_number,
            labels: (labelsByPull[pull_number] || []).map((name) => ({ name })),
          },
        }),
      },
    },
  }
}

async function main() {
  const repo = { owner: 'IvanLi-CN', repo: 'dockrev' }

  const labelGithub = makeLabelGithub({
    labelsByPull: {
      101: ['type:patch', 'channel:stable'],
      102: ['type:minor', 'channel:rc'],
    },
    commitPullsBySha: {
      'sha-label-exact': [
        { number: 101, state: 'open', base: { ref: 'main' } },
      ],
      'sha-label-extra': [
        { number: 101, state: 'open', base: { ref: 'main' } },
        { number: 999, state: 'open', base: { ref: 'main' } },
      ],
    },
  })

  let core = await runGithubScript(labelScript, {
    context: {
      eventName: 'pull_request_target',
      repo,
      payload: { pull_request: { number: 101 } },
      ref: 'refs/heads/main',
      sha: 'sha-pr',
    },
    github: labelGithub,
  })
  assert(core.failed === null, `label gate pull_request_target should pass, got: ${core.failed}`)

  core = await runGithubScript(labelScript, {
    context: {
      eventName: 'merge_group',
      repo,
      payload: {
        merge_group: {
          base_ref: 'refs/heads/main',
          head_ref: 'gh-readonly-queue/main/pr-101-deadbeef',
          head_sha: 'sha-label-exact',
          head_commit: { message: 'merge queue contains Fix #999' },
        },
      },
      ref: 'refs/heads/gh-readonly-queue/main/pr-101-deadbeef',
      sha: 'sha-label-exact',
    },
    github: labelGithub,
  })
  assert(core.failed === null, `label gate merge_group with noisy commit message should pass, got: ${core.failed}`)

  core = await runGithubScript(labelScript, {
    context: {
      eventName: 'merge_group',
      repo,
      payload: {
        merge_group: {
          base_ref: 'refs/heads/main',
          head_ref: 'gh-readonly-queue/main/pr-101-deadbeef',
          head_sha: 'sha-label-extra',
        },
      },
      ref: 'refs/heads/gh-readonly-queue/main/pr-101-deadbeef',
      sha: 'sha-label-extra',
    },
    github: labelGithub,
  })
  assert(core.failed === null, `label gate merge_group should ignore unrelated associated pulls, got: ${core.failed}`)
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack : error)
  process.exit(1)
})
NODE
}

run_live_quality_gates() {
  local branch="$1"
  local expected_status="$2"
  set +e
  local output
  output="$({
    QUALITY_GATES_LIVE_RULES_MODE=require \
      GITHUB_API_URL="http://127.0.0.1:${api_port}" \
      GITHUB_REPOSITORY="IvanLi-CN/dockrev" \
      GITHUB_TOKEN="x" \
      python3 ./.github/scripts/check-live-quality-gates.py \
        --declaration .github/quality-gates.json \
        --repo IvanLi-CN/dockrev \
        --branch "${branch}"
  } 2>&1)"
  local code=$?
  set -e
  if [[ "${expected_status}" == "ok" ]]; then
    if [[ "${code}" -ne 0 ]]; then
      echo "[contract-check] live-quality-gates branch=${branch} expected ok, got ${code}" >&2
      echo "${output}" >&2
      exit 1
    fi
  else
    if [[ "${code}" -eq 0 ]]; then
      echo "[contract-check] live-quality-gates branch=${branch} expected fail, got success" >&2
      echo "${output}" >&2
      exit 1
    fi
  fi
}

run_label_gate() {
  local pr_number="$1"
  local expected_status="$2"
  set +e
  local output
  output="$({
    GITHUB_API_URL="http://127.0.0.1:${api_port}" \
      GITHUB_REPOSITORY="IvanLi-CN/dockrev" \
      GITHUB_TOKEN="x" \
      PR_NUMBER="${pr_number}" \
      bash ./.github/scripts/label-gate.sh
  } 2>&1)"
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

echo "[contract-check] inline github-script scenarios"
run_inline_workflow_contract_checks

echo "[contract-check] live quality-gates scenarios"
run_live_quality_gates main ok
run_live_quality_gates main-missing-review fail
run_live_quality_gates main-stale-check fail

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
