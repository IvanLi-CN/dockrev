#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

echo "[contract-check] syntax + workflow yaml parse"
bash -n \
  .github/scripts/label-gate.sh \
  .github/scripts/release-intent.sh \
  .github/scripts/compute-version.sh \
  .github/scripts/test-release-snapshot.sh \
  .github/scripts/deploy-smoke.sh \
  .github/scripts/storybook-ci-check.sh
python3 -m py_compile \
  .github/scripts/check-live-quality-gates.py \
  .github/scripts/release_snapshot.py \
  .github/scripts/resolve-ci-scope.py \
  .github/scripts/resolve-storybook-matrix.py \
  .github/scripts/release_source_gate.py \
  .github/scripts/verify_ci_gate_metrics.py \
  .github/scripts/test_ci_release_gate.py \
  .github/scripts/run_ci_gate_validation.py
ruby -e 'require "yaml"; ARGV.each { |path| YAML.load_file(path) }' \
  .github/workflows/label-gate.yml \
  .github/workflows/review-policy.yml \
  .github/workflows/ci-pr.yml \
  .github/workflows/ci-main.yml \
  .github/workflows/source-build-release-gate.yml \
  .github/workflows/ci-gate-verification.yml \
  .github/workflows/release.yml

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

ensure_fixed_absent() {
  local needle="$1"
  local file="$2"
  if search_fixed "${needle}" "${file}"; then
    echo "[contract-check] unexpected text '${needle}' in ${file}" >&2
    exit 1
  fi
}

echo "[contract-check] release workflow rc gating invariants"
search_regex "group:[[:space:]]*release-main" .github/workflows/release.yml
search_fixed "head_sha:" .github/workflows/release.yml
search_fixed "admin_action:" .github/workflows/release.yml
search_fixed "packages: read" .github/workflows/release.yml
search_fixed "python3 .github/scripts/release_snapshot.py ensure \\" .github/workflows/release.yml
search_fixed "python3 .github/scripts/release_snapshot.py export \\" .github/workflows/release.yml
search_fixed "python3 .github/scripts/release_snapshot.py reconcile-publications \\" .github/workflows/release.yml
search_fixed "python3 .github/scripts/release_snapshot.py next-pending \\" .github/workflows/release.yml
search_fixed "python3 .workflow-src/.github/scripts/release_snapshot.py next-pending \\" .github/workflows/release.yml
search_fixed "python3 .github/scripts/release_snapshot.py record-override \\" .github/workflows/release.yml
search_fixed "if: github.event_name != 'workflow_dispatch' || inputs.admin_action != 'release'" .github/workflows/release.yml
search_fixed "if [ \"\${{ github.event_name }}\" = \"workflow_dispatch\" ] && [ \"\${{ inputs.admin_action }}\" = \"release\" ]; then" .github/workflows/release.yml
search_fixed "echo \"target_sha=\${REQUESTED_SHA}\" >> \"\$GITHUB_OUTPUT\"" .github/workflows/release.yml
search_fixed "Skipped release targets cannot be released manually" .github/workflows/release.yml
search_fixed 'DOCKREV_TAGS_CSV: ${{ needs.prepare.outputs.tags_csv }}' .github/workflows/release.yml
search_fixed 'SUPERVISOR_TAGS_CSV: ${{ needs.prepare.outputs.supervisor_tags_csv }}' .github/workflows/release.yml
search_fixed "inputs: { head_sha: nextSha, admin_action: 'release', override_reason: '' }" .github/workflows/release.yml
search_fixed "Create and push tag" .github/workflows/release.yml
search_fixed 'git push origin "refs/tags/${RELEASE_TAG}:refs/tags/${RELEASE_TAG}"' .github/workflows/release.yml
search_fixed "makeLatest: \${{ needs.prepare.outputs.publish_latest }}" .github/workflows/release.yml
ensure_fixed_absent 'commit: ${{ env.TARGET_SHA }}' .github/workflows/release.yml
python3 - <<'PY'
from pathlib import Path
text = Path('.github/workflows/release.yml').read_text()
needle = "prerelease: ${{ env.RELEASE_CHANNEL == 'rc' }}"
if needle not in text:
    raise SystemExit('[contract-check] expected prerelease gate in release workflow')
latest_needle = "makeLatest: ${{ needs.prepare.outputs.publish_latest }}"
if latest_needle not in text:
    raise SystemExit('[contract-check] expected explicit makeLatest wiring in release workflow')
workflow_source_next_pending = "python3 .workflow-src/.github/scripts/release_snapshot.py next-pending \\"
if workflow_source_next_pending not in text:
    raise SystemExit('[contract-check] expected queue continuation to use workflow-source release_snapshot helper')
prepare_permissions = """    permissions:
      contents: write
      packages: read
      pull-requests: read"""
if prepare_permissions not in text:
    raise SystemExit('[contract-check] prepare job must retain packages: read for GHCR-backed reconciliation')

reconcile_step = "Reconcile historical published backlog"
select_pending_step = "Select pending release target"
manual_release_bypass = "if: github.event_name != 'workflow_dispatch' || inputs.admin_action != 'release'"
tag_step = "Create and push tag"
release_step = "Create or update GitHub Release + upload assets"
comment_step = "Upsert and verify release-version comment on source PR"
ledger_step = "Record release publication ledger"
reconcile_idx = text.find(reconcile_step)
select_pending_idx = text.find(select_pending_step)
tag_idx = text.find(tag_step)
release_idx = text.find(release_step)
comment_idx = text.find(comment_step)
ledger_idx = text.find(ledger_step)
if min(reconcile_idx, select_pending_idx, tag_idx, release_idx, comment_idx, ledger_idx) == -1:
    raise SystemExit('[contract-check] expected reconcile/select-pending/tag/release/comment/ledger steps in release workflow')
if not (reconcile_idx < select_pending_idx):
    raise SystemExit('[contract-check] release workflow must reconcile historical publications before selecting next pending target')
if manual_release_bypass not in text:
    raise SystemExit('[contract-check] manual admin_action=release must bypass backlog reconciliation')
if not (tag_idx < release_idx < comment_idx < ledger_idx):
    raise SystemExit('[contract-check] release workflow must run tag -> release -> PR comment -> publication ledger in order')
PY
ruby -ryaml -e '
workflow = YAML.load_file(".github/workflows/release.yml")
top_permissions = workflow.fetch("permissions", {})
publish_permissions = workflow.fetch("jobs").fetch("publish").fetch("permissions", {})
jobs = workflow.fetch("jobs")

%w[build-web build-binaries-amd64 build-binaries-arm64 publish cleanup-artifacts].each do |job_name|
  needs = Array(jobs.fetch(job_name).fetch("needs"))
  abort "[contract-check] #{job_name} must wait for source-gate" unless needs.include?("source-gate")
end
abort "[contract-check] source-gate must have actions: read" unless jobs.fetch("source-gate").fetch("permissions")["actions"] == "read"

abort "[contract-check] release workflow top-level permissions must keep issues: write" unless top_permissions["issues"] == "write"
abort "[contract-check] release workflow top-level permissions must keep pull-requests: write" unless top_permissions["pull-requests"] == "write"
abort "[contract-check] release publish job permissions must keep issues: write" unless publish_permissions["issues"] == "write"
abort "[contract-check] release publish job permissions must keep pull-requests: write" unless publish_permissions["pull-requests"] == "write"
'

ruby -ryaml -e '
workflow = YAML.load_file(".github/workflows/ci-main.yml")
jobs = workflow.fetch("jobs")
required = jobs.fetch("frontend-storybook-test-required")
abort "[contract-check] Storybook required check name changed" unless required.fetch("name") == "Frontend Storybook test (main)"
abort "[contract-check] Storybook required check must aggregate shards and coverage" unless Array(required.fetch("needs")).sort == %w[changes frontend-storybook-test storybook-coverage-summary].sort
abort "[contract-check] fast gate must wait for the stable Storybook required check" unless Array(jobs.fetch("fast-gate-verdict").fetch("needs")).include?("frontend-storybook-test-required")
abort "[contract-check] release snapshot must wait for the stable Storybook required check" unless Array(jobs.fetch("release-snapshot").fetch("needs")).include?("frontend-storybook-test-required")
'

echo "[contract-check] CI duration optimization invariants"
search_fixed "source-gate:" .github/workflows/release.yml
search_fixed "python3 .github/scripts/release_source_gate.py wait \\" .github/workflows/release.yml
search_fixed "WORKFLOW_FILE = \"source-build-release-gate.yml\"" .github/scripts/release_source_gate.py
search_fixed "FAST_WORKFLOW_FILE = \"ci-main.yml\"" .github/scripts/release_source_gate.py
search_fixed "VERIFICATION_WORKFLOW_FILE = \"ci-gate-verification.yml\"" .github/scripts/release_source_gate.py
search_fixed "validate_push_attestation" .github/scripts/release_source_gate.py
search_fixed "download_attestation" .github/scripts/release_source_gate.py
search_fixed "target: runtime" .github/workflows/source-build-release-gate.yml
search_fixed "target: runtime-supervisor" .github/workflows/source-build-release-gate.yml
search_fixed "load: true" .github/workflows/source-build-release-gate.yml
search_fixed "push: false" .github/workflows/source-build-release-gate.yml
search_fixed "docker/build-push-action@v6" .github/workflows/source-build-release-gate.yml
search_fixed "crazy-max/ghaction-github-runtime@v4" .github/workflows/source-build-release-gate.yml
search_fixed 'dockrev-source-runtime-v1${{ env.SOURCE_CACHE_SUFFIX }}' .github/workflows/source-build-release-gate.yml
search_fixed 'dockrev-source-supervisor-v1${{ env.SOURCE_CACHE_SUFFIX }}' .github/workflows/source-build-release-gate.yml
search_fixed "DOCKREV_DEPLOY_SMOKE_USE_LOADED_IMAGES: \"1\"" .github/workflows/source-build-release-gate.yml
search_regex "docker compose .*up -d --no-build" .github/scripts/deploy-smoke.sh
search_fixed "SOURCE_CACHE_SUFFIX" .github/workflows/source-build-release-gate.yml
search_fixed '"publish": False' .github/workflows/source-build-release-gate.yml
search_fixed "target_sha:" .github/workflows/ci-gate-verification.yml
search_fixed "force_full: true" .github/workflows/ci-gate-verification.yml
search_fixed "verification_mode: true" .github/workflows/ci-gate-verification.yml
search_fixed '"publish": False' .github/workflows/ci-gate-verification.yml
search_fixed "TOTAL_DISPATCHES = 17" .github/scripts/run_ci_gate_validation.py
search_fixed "--signal=TERM" .github/scripts/run_ci_gate_validation.py
search_fixed "timeout-seconds=720" .github/scripts/run_ci_gate_validation.py
search_fixed "interval-seconds=15" .github/scripts/run_ci_gate_validation.py
search_fixed "capture=True" .github/scripts/run_ci_gate_validation.py
search_fixed "17-run validation budget of 204 minutes has elapsed" .github/scripts/run_ci_gate_validation.py
search_fixed "output-dir must not already exist" .github/scripts/run_ci_gate_validation.py
search_fixed "final ten warm samples" .github/scripts/run_ci_gate_validation.py
search_fixed "storybook-coverage-summary" .github/workflows/ci-main.yml
search_fixed "frontend-storybook-test-required:" .github/workflows/ci-main.yml
search_fixed "name: Frontend Storybook test (main)" .github/workflows/ci-main.yml
search_fixed "selectSmokeShard" web/scripts/test-storybook.mjs
search_fixed "DOCKREV_STORYBOOK_ROLLBACK_RACE_PASSED" web/scripts/test-storybook.mjs
search_fixed "DOCKREV_STORYBOOK_ROLLBACK_RACE_PASSED=1" .github/scripts/storybook-ci-check.sh
search_fixed "verify-storybook-coverage.mjs" .github/workflows/ci-main.yml
search_fixed "resolve-storybook-matrix.py" .github/workflows/ci-main.yml
search_fixed ".github/storybook-shards.json" .github/scripts/resolve-storybook-matrix.py

echo "[contract-check] quality-gate workflow invariants"
search_regex "^[[:space:]]*pull_request_target:" .github/workflows/label-gate.yml
search_regex "^[[:space:]]*merge_group:" .github/workflows/label-gate.yml
search_regex "pull-requests:[[:space:]]*read" .github/workflows/label-gate.yml
search_regex "uses:[[:space:]]*actions/github-script@" .github/workflows/label-gate.yml
search_regex "listFiles" .github/workflows/label-gate.yml
search_regex "Release-infra-only PRs must use type:skip or type:docs" .github/workflows/label-gate.yml
search_regex "Release-enabled PRs must not touch \\.github/workflows/\\*\\*" .github/workflows/label-gate.yml
search_regex "resolveMergeGroupPullNumbers" .github/workflows/label-gate.yml
search_regex "GET /repos/\{owner\}/\{repo\}/commits/\{commit_sha\}/pulls" .github/workflows/label-gate.yml
search_regex "context\.eventName === 'merge_group'" .github/workflows/label-gate.yml
search_regex "context\.payload\.pull_request\?\.number" .github/workflows/label-gate.yml
ensure_regex_absent "^[[:space:]]*pull_request:" .github/workflows/label-gate.yml
ensure_regex_absent "run:[[:space:]]*bash[[:space:]]+\./\.github/scripts/label-gate\.sh" .github/workflows/label-gate.yml
ensure_regex_absent "head_commit\?\.message" .github/workflows/label-gate.yml

search_regex "^[[:space:]]*pull_request_target:" .github/workflows/review-policy.yml
search_regex "^[[:space:]]*pull_request_review:" .github/workflows/review-policy.yml
search_regex "^[[:space:]]*merge_group:" .github/workflows/review-policy.yml
search_regex "pull-requests:[[:space:]]*read" .github/workflows/review-policy.yml
search_regex "uses:[[:space:]]*actions/github-script@" .github/workflows/review-policy.yml
search_regex "resolveMergeGroupPullNumbers" .github/workflows/review-policy.yml
search_regex "getCollaboratorPermissionLevel" .github/workflows/review-policy.yml
search_regex "listReviews" .github/workflows/review-policy.yml
ensure_regex_absent "^[[:space:]]*pull_request:" .github/workflows/review-policy.yml
ensure_regex_absent "statuses:[[:space:]]*write" .github/workflows/review-policy.yml
ensure_regex_absent "createCommitStatus" .github/workflows/review-policy.yml

search_fixed "Live quality-gates check" .github/workflows/ci-pr.yml
search_fixed "Live quality-gates check" .github/workflows/ci-main.yml
live_gate_require_count="$(
  (
    count_fixed_lines 'QUALITY_GATES_LIVE_RULES_MODE: require' .github/workflows/ci-pr.yml
    count_fixed_lines 'QUALITY_GATES_LIVE_RULES_MODE: require' .github/workflows/ci-main.yml
  ) | awk '{sum += $1} END {print sum + 0}'
)"
if [[ "${live_gate_require_count}" -ne 2 ]]; then
  echo "[contract-check] expected authenticated live quality-gates steps in CI workflows, got ${live_gate_require_count}" >&2
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
assert policy['branch_protection'].get('require_merge_queue') is False, 'dockrev should not require merge queue by default'
assert review['mode'] == 'conditional-required', 'review_policy.mode must stay conditional-required'
assert review['required_approvals'] == 1, 'review_policy.required_approvals must stay 1'
assert review['exempt_repository_owner'] is True, 'repository owner must stay exempt'
assert review['exempt_author_permissions'] == ['admin', 'maintain'], 'exempt author permissions drifted'
assert review['allowed_reviewer_permissions'] == ['write', 'maintain', 'admin'], 'allowed reviewer permissions drifted'
assert enforcement['mode'] == 'required-check', 'review policy enforcement must stay workflow-backed for conditional exemptions'
assert enforcement['check_name'] == 'Review Policy Gate', 'review policy check name drifted'
assert 'Review Policy Gate' in required_checks, 'Review Policy Gate must stay required'
assert 'Review Policy Gate' not in informational_checks, 'Review Policy Gate must not be informational'
assert any(item.get('workflow') == 'Review Policy' for item in expected_pr_workflows), 'Review Policy workflow must stay declared'
assert 'Release intent label gate' in required_checks, 'label gate must stay required'
PY

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
    "Review Policy Gate",
    "Release intent label gate",
    "Detect changes",
    "Lint & Checks",
    "Worktree bootstrap smoke",
    "Backend Tests",
    "Frontend lint + build",
    "Frontend Storybook build",
    "Frontend Storybook test",
    "Release build check (PR)",
]

def make_review_rules(required_approvals):
    return [
        {
            "type": "pull_request",
            "parameters": {
                "required_approving_review_count": required_approvals,
                "dismiss_stale_reviews_on_push": False,
                "require_code_owner_review": False,
                "require_last_push_approval": False,
                "required_review_thread_resolution": False,
                "allowed_merge_methods": ["merge", "squash", "rebase"],
            },
        },
        {"type": "required_signatures"},
        {"type": "non_fast_forward"},
    ]


def make_required_checks_rules(checks=None):
    selected_checks = REQUIRED_CHECKS if checks is None else checks
    return [
        {
            "type": "required_status_checks",
            "parameters": {
                "strict_required_status_checks_policy": True,
                "required_status_checks": [
                    {"context": context, "integration_id": 15368}
                    for context in selected_checks
                ],
            },
        }
    ]


RULES_BY_BRANCH = {
    "main": make_review_rules(0) + make_required_checks_rules(),
    "main-mirror": make_review_rules(0) + make_required_checks_rules(),
    "main-extra-review": make_review_rules(1) + make_required_checks_rules(),
    "main-missing-check": make_review_rules(0) + make_required_checks_rules(REQUIRED_CHECKS[1:]),
    "main-unexpected-merge-queue": make_review_rules(0) + make_required_checks_rules() + [{"type": "merge_queue"}],
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
  local review_script="${tmp_dir}/review-policy.inline.js"
  extract_github_script \
    .github/workflows/label-gate.yml \
    label-gate \
    "Validate release intent + channel labels" \
    "${label_script}"
  extract_github_script \
    .github/workflows/review-policy.yml \
    review-policy \
    "Evaluate review policy" \
    "${review_script}"

  node - "${label_script}" "${review_script}" <<'NODE'
const fs = require('fs')

const labelScript = fs.readFileSync(process.argv[2], 'utf8')
const reviewScript = fs.readFileSync(process.argv[3], 'utf8')

function createCore(inputs = {}) {
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
    getInput(name) { return inputs[name] || '' },
  }
}

async function runGithubScript(script, { context, github, env = {}, inputs = {} }) {
  const core = createCore(inputs)
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

function makeLabelGithub({ labelsByPull, filesByPull, commitPullsBySha }) {
  const listFiles = async ({ pull_number }) => ({
    data: (filesByPull[pull_number] || []).map((filename) => ({ filename })),
  })

  return {
    paginate: async (route, params) => {
      if (typeof route === 'string' && route.includes('/commits/{commit_sha}/pulls')) {
        return commitPullsBySha[params.commit_sha] || []
      }
      if (route === listFiles) {
        return (filesByPull[params.pull_number] || []).map((filename) => ({ filename }))
      }
      throw new Error(`unexpected label-gate paginate route: ${String(route)}`)
    },
    rest: {
      pulls: {
        listFiles,
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

function makeReviewGithub({ pullsByNumber, reviewsByPull, permissionsByUser, commitPullsBySha }) {
  const listReviews = async ({ pull_number }) => ({
    data: (reviewsByPull[pull_number] || []).map((review) => ({ ...review })),
  })

  return {
    paginate: async (route, params) => {
      if (typeof route === 'string' && route.includes('/commits/{commit_sha}/pulls')) {
        return commitPullsBySha[params.commit_sha] || []
      }
      if (route === listReviews) {
        return (reviewsByPull[params.pull_number] || []).map((review) => ({ ...review }))
      }
      throw new Error(`unexpected review-policy paginate route: ${String(route)}`)
    },
    rest: {
      pulls: {
        listReviews,
        get: async ({ pull_number }) => {
          const pull = pullsByNumber[pull_number]
          if (!pull) {
            throw new Error(`missing pull ${pull_number}`)
          }
          return {
            data: {
              number: pull_number,
              user: { login: pull.author },
              head: { sha: pull.headSha },
              base: { ref: pull.baseRef || 'main' },
            },
          }
        },
      },
      repos: {
        getCollaboratorPermissionLevel: async ({ username }) => ({
          data: { permission: permissionsByUser[username] || 'none' },
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
      107: ['type:patch', 'channel:stable'],
      108: ['type:docs', 'channel:stable'],
      109: ['type:patch', 'channel:stable'],
      999: ['type:docs', 'channel:stable'],
      998: ['type:minor'],
    },
    filesByPull: {
      101: ['crates/dockrev-api/src/lib.rs'],
      102: ['web/src/App.tsx'],
      107: ['.github/scripts/release_snapshot.py', '.github/scripts/test-release-snapshot.sh'],
      108: ['.github/workflows/release.yml'],
      109: ['web/src/App.tsx', '.github/workflows/release.yml'],
      999: ['README.md'],
      998: ['crates/dockrev-api/src/lib.rs'],
    },
    commitPullsBySha: {
      'sha-label-exact': [
        { number: 101, state: 'open', base: { ref: 'main' } },
      ],
      'sha-label-pass-multi': [
        { number: 101, state: 'open', base: { ref: 'main' } },
        { number: 999, state: 'open', base: { ref: 'main' } },
      ],
      'sha-label-invalid-associated': [
        { number: 101, state: 'open', base: { ref: 'main' } },
        { number: 998, state: 'open', base: { ref: 'main' } },
      ],
      'sha-label-mismatch': [
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
  assert(core.failed === null, `label gate pull_request should pass, got: ${core.failed}`)

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
          head_ref: 'gh-readonly-queue/main/pr-999-cafebabe',
          head_sha: 'sha-label-pass-multi',
        },
      },
      ref: 'refs/heads/gh-readonly-queue/main/pr-999-cafebabe',
      sha: 'sha-label-pass-multi',
    },
    github: labelGithub,
  })
  assert(core.failed === null, `label gate merge_group should pass when the proven PR set exactly matches and every associated PR is valid, got: ${core.failed}`)

  core = await runGithubScript(labelScript, {
    context: {
      eventName: 'merge_group',
      repo,
      payload: {
        merge_group: {
          base_ref: 'refs/heads/main',
          head_ref: 'gh-readonly-queue/main/pr-998-cafebabe',
          head_sha: 'sha-label-invalid-associated',
        },
      },
      ref: 'refs/heads/gh-readonly-queue/main/pr-998-cafebabe',
      sha: 'sha-label-invalid-associated',
    },
    github: labelGithub,
  })
  assert(core.failed !== null && core.failed.includes('PR #998'), `label gate merge_group should fail when any associated PR in the merge-group commit is invalid, got: ${core.failed}`)

  core = await runGithubScript(labelScript, {
    context: {
      eventName: 'merge_group',
      repo,
      payload: {
        merge_group: {
          base_ref: 'refs/heads/main',
          head_ref: 'gh-readonly-queue/main/pr-101-deadbeef',
          head_sha: 'sha-label-mismatch',
        },
      },
      ref: 'refs/heads/gh-readonly-queue/main/pr-101-deadbeef',
      sha: 'sha-label-mismatch',
    },
    github: labelGithub,
  })
  assert(core.failed !== null && core.failed.includes('mismatch'), `label gate merge_group should fail when parsed PRs are not proven by commit metadata, got: ${core.failed}`)

  core = await runGithubScript(labelScript, {
    context: {
      eventName: 'pull_request_target',
      repo,
      payload: { pull_request: { number: 107 } },
      ref: 'refs/heads/main',
      sha: 'sha-pr-infra-only',
    },
    github: labelGithub,
  })
  assert(core.failed !== null && core.failed.includes('Release-infra-only PRs must use type:skip or type:docs'), `label gate should fail infra-only release-enabled PRs, got: ${core.failed}`)

  core = await runGithubScript(labelScript, {
    context: {
      eventName: 'pull_request_target',
      repo,
      payload: { pull_request: { number: 108 } },
      ref: 'refs/heads/main',
      sha: 'sha-pr-infra-docs',
    },
    github: labelGithub,
  })
  assert(core.failed === null, `label gate should allow workflow-only docs/skip PRs, got: ${core.failed}`)

  core = await runGithubScript(labelScript, {
    context: {
      eventName: 'pull_request_target',
      repo,
      payload: { pull_request: { number: 109 } },
      ref: 'refs/heads/main',
      sha: 'sha-pr-mixed-workflow',
    },
    github: labelGithub,
  })
  assert(core.failed !== null && core.failed.includes('must not touch .github/workflows/**'), `label gate should fail release-enabled PRs that touch workflows, got: ${core.failed}`)

  const reviewGithub = makeReviewGithub({
    pullsByNumber: {
      201: { author: 'IvanLi-CN', headSha: 'sha-owner', baseRef: 'main' },
      202: { author: 'alice', headSha: 'sha-alice', baseRef: 'main' },
      203: { author: 'bob', headSha: 'sha-bob', baseRef: 'main' },
    },
    reviewsByPull: {
      202: [],
      203: [
        {
          user: { login: 'carol' },
          state: 'APPROVED',
          submitted_at: '2026-03-11T11:00:00Z',
        },
      ],
    },
    permissionsByUser: {
      alice: 'write',
      bob: 'write',
      carol: 'write',
    },
    commitPullsBySha: {
      'sha-review-pass': [
        { number: 201, state: 'open', base: { ref: 'main' } },
        { number: 203, state: 'open', base: { ref: 'main' } },
      ],
      'sha-review-fail': [
        { number: 201, state: 'open', base: { ref: 'main' } },
        { number: 202, state: 'open', base: { ref: 'main' } },
      ],
    },
  })

  core = await runGithubScript(reviewScript, {
    context: {
      eventName: 'pull_request_target',
      repo,
      payload: { pull_request: { number: 201 } },
      ref: 'refs/heads/main',
      sha: 'sha-owner',
    },
    github: reviewGithub,
    env: {
      REVIEW_POLICY_REQUIRED_APPROVALS: '1',
      REVIEW_POLICY_EXEMPT_PERMISSIONS: '["admin","maintain"]',
      REVIEW_POLICY_REVIEWER_PERMISSIONS: '["write","maintain","admin"]',
      REVIEW_POLICY_EXEMPT_REPOSITORY_OWNER: 'true',
    },
  })
  assert(core.failed === null, `review policy should exempt repository owner, got: ${core.failed}`)

  core = await runGithubScript(reviewScript, {
    context: {
      eventName: 'pull_request_target',
      repo,
      payload: { pull_request: { number: 202 } },
      ref: 'refs/heads/main',
      sha: 'sha-alice',
    },
    github: reviewGithub,
    env: {
      REVIEW_POLICY_REQUIRED_APPROVALS: '1',
      REVIEW_POLICY_EXEMPT_PERMISSIONS: '["admin","maintain"]',
      REVIEW_POLICY_REVIEWER_PERMISSIONS: '["write","maintain","admin"]',
      REVIEW_POLICY_EXEMPT_REPOSITORY_OWNER: 'true',
    },
  })
  assert(core.failed !== null && core.failed.includes('PR #202'), `review policy should fail unreviewed non-exempt author, got: ${core.failed}`)

  core = await runGithubScript(reviewScript, {
    context: {
      eventName: 'pull_request_target',
      repo,
      payload: { pull_request: { number: 203 } },
      ref: 'refs/heads/main',
      sha: 'sha-bob',
    },
    github: reviewGithub,
    env: {
      REVIEW_POLICY_REQUIRED_APPROVALS: '1',
      REVIEW_POLICY_EXEMPT_PERMISSIONS: '["admin","maintain"]',
      REVIEW_POLICY_REVIEWER_PERMISSIONS: '["write","maintain","admin"]',
      REVIEW_POLICY_EXEMPT_REPOSITORY_OWNER: 'true',
    },
  })
  assert(core.failed === null, `review policy should pass with one valid approval, got: ${core.failed}`)

  core = await runGithubScript(reviewScript, {
    context: {
      eventName: 'merge_group',
      repo,
      payload: {
        merge_group: {
          base_ref: 'refs/heads/main',
          head_ref: 'gh-readonly-queue/main/pr-203-cafebabe',
          head_sha: 'sha-review-pass',
        },
      },
      ref: 'refs/heads/gh-readonly-queue/main/pr-203-cafebabe',
      sha: 'sha-review-pass',
    },
    github: reviewGithub,
    env: {
      REVIEW_POLICY_REQUIRED_APPROVALS: '1',
      REVIEW_POLICY_EXEMPT_PERMISSIONS: '["admin","maintain"]',
      REVIEW_POLICY_REVIEWER_PERMISSIONS: '["write","maintain","admin"]',
      REVIEW_POLICY_EXEMPT_REPOSITORY_OWNER: 'true',
    },
  })
  assert(core.failed === null, `review policy merge_group should pass when every associated PR passes, got: ${core.failed}`)

  core = await runGithubScript(reviewScript, {
    context: {
      eventName: 'merge_group',
      repo,
      payload: {
        merge_group: {
          base_ref: 'refs/heads/main',
          head_ref: 'gh-readonly-queue/main/pr-202-cafebabe',
          head_sha: 'sha-review-fail',
        },
      },
      ref: 'refs/heads/gh-readonly-queue/main/pr-202-cafebabe',
      sha: 'sha-review-fail',
    },
    github: reviewGithub,
    env: {
      REVIEW_POLICY_REQUIRED_APPROVALS: '1',
      REVIEW_POLICY_EXEMPT_PERMISSIONS: '["admin","maintain"]',
      REVIEW_POLICY_REVIEWER_PERMISSIONS: '["write","maintain","admin"]',
      REVIEW_POLICY_EXEMPT_REPOSITORY_OWNER: 'true',
    },
  })
  assert(core.failed !== null && core.failed.includes('PR #202'), `review policy merge_group should fail when any associated PR is unapproved, got: ${core.failed}`)
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

echo "[contract-check] release snapshot self-test"
bash ./.github/scripts/test-release-snapshot.sh

echo "[contract-check] live quality-gates scenarios"
run_live_quality_gates main ok
python3 - <<'PY' "${tmp_dir}/quality-gates.multi-branch.json"
from __future__ import annotations
import json
import sys
from pathlib import Path

out = Path(sys.argv[1])
data = json.loads(Path('.github/quality-gates.json').read_text())
data['policy']['branch_protection']['protected_branches'] = ['main', 'main-mirror']
out.write_text(json.dumps(data, indent=2) + '\n')
PY
QUALITY_GATES_LIVE_RULES_MODE=require \
  GITHUB_API_URL="http://127.0.0.1:${api_port}" \
  GITHUB_REPOSITORY="IvanLi-CN/dockrev" \
  GITHUB_TOKEN="x" \
  python3 ./.github/scripts/check-live-quality-gates.py \
    --declaration "${tmp_dir}/quality-gates.multi-branch.json" \
    --repo IvanLi-CN/dockrev >/dev/null
run_live_quality_gates main-extra-review fail
run_live_quality_gates main-missing-check fail
run_live_quality_gates main-unexpected-merge-queue fail

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
