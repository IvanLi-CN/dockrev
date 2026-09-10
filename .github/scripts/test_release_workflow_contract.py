#!/usr/bin/env python3
"""Static contract checks for the trusted release workflow topology."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap


ROOT = Path(__file__).resolve().parents[2]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


label_gate = text(".github/workflows/label-gate.yml")
preparation = text(".github/workflows/release-preparation.yml")
completion = text(".github/workflows/release-completion-pr.yml")
release = text(".github/workflows/release.yml")
notify = text(".github/workflows/notify-release-failure.yml")
ci_pr = text(".github/workflows/ci-pr.yml")

assert "name: Label Gate" in label_gate and "pull_request:" in label_gate and "pull_request_target:" in label_gate
assert "ref: ${{ github.event.pull_request.base.sha }}" in label_gate
assert "ref: ${{ github.event.pull_request.head.sha }}" not in label_gate
assert "github.event_name == 'pull_request_target'" in label_gate
assert "github.event.pull_request.base.sha == '759b0cf9c0d5a57be1010e74480cbb5ae713433c'" in label_gate
assert "cancel-in-progress: ${{ github.event_name == 'pull_request' }}" in label_gate
assert "name: Release completion" in completion
assert "pull_request_target:" in completion and "pull_request:" in completion
assert "ref: ${{ github.event.pull_request.base.sha }}" in completion
assert "759b0cf9c0d5a57be1010e74480cbb5ae713433c" in completion
assert "workflows: [\"CI (PR)\", \"Label Gate\"]" in preparation
assert "group: release-preparation-${{ inputs.pr_number || github.event.workflow_run.pull_requests[0].number || github.run_id }}" in preparation
assert "name: Prepare PR VERSION identity" in preparation
assert "release-reservation" in text(".github/scripts/release_preparation.py")
assert "actions: read" in preparation
assert "checks: read" in preparation
assert "checks: read" in completion
assert "checks: read" in preparation
assert "checks: read" in completion
assert "ref: main" in preparation
assert "createCommitOnBranch" in text(".github/scripts/release_preparation.py")
assert "branches: [main]" in release and "merge_sha:" in release and "recovery_reason:" in release
assert "workflow_dispatch requires an existing release-enabled immutable identity" in release
assert 'gh api --paginate --slurp "repos/${GITHUB_REPOSITORY}/actions/workflows/release.yml/runs?head_sha=${MERGE_SHA}&per_page=100"' in release
assert "release-failure-context-" in release and "workflow_dispatch merge_sha=" in release
assert "create VERSION-only release PR Covered-Product-Merge-SHA=" in release
assert "prior failed automatic Release run" in release
assert "path: release-assets" in release and 'chmod +x "${source}"' in release
assert "needs.identity.result == 'failure'" in release
assert "release-identity-failure-context-" in release
assert 'INTENT_TYPE: ${{ steps.resolve.outputs.type || \'\' }}' in release
assert 'INTENT_CHANNEL: ${{ steps.resolve.outputs.channel || \'\' }}' in release
assert "Release failure context requires a verified release intent" in release
assert "refusing to fabricate identity" in release
assert '"type": "patch"' not in release.split("Capture identity-resolution failure context", 1)[1].split("Upload identity-resolution failure context", 1)[0]
assert "Capture resolved identity step failure context" in release
assert "steps.resolve.outputs.release_enabled == 'true'" in release
assert "resolver-error: retry Release workflow" in release
assert '"identity_failure_kind": "identity-step-failure"' in text(".github/scripts/release_failure_context.py")
assert "steps.upload_identity.outcome == 'failure'" in release
assert "release-publish-${{ needs.identity.outputs.merge_sha }}" in release
assert "release-latest-lock" in release
assert "Release-Latest-Lock-Run:" in release
assert "Release-Latest-Lock-State: acquired" in release
assert "--method PATCH" in release and "-F force=false" in release
assert "/actions/runs/${owner_run}/attempts/${owner_attempt}" in release
assert "HTTP 404|Not Found|404" in release
assert release.index("trap release_lock EXIT") > release.index("while true; do")
assert "git/ref/heads/${lock_ref_name}" in release
assert "git/refs/heads/${lock_ref_name}" in release
assert "Release-Latest-Lock-State: released" in release
assert 'lock_error_dir="${RUNNER_TEMP:-/tmp}/dockrev-release-latest-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}"' in release
assert "tr '[:upper:]' '[:lower:]'" in release
assert "overwrite: true" in release
assert "github.run_attempt" in release
assert "oidrune/.github/workflows/notify.yml@" in notify
assert "name: release-failure-context-${{ github.event.workflow_run.id }}-${{ github.event.workflow_run.run_attempt }}" in notify
assert "EXPECTED_RELEASE_RUN_ID" in notify
assert "required_secrets" in text(".github/release-failure-notification.json")
assert "release-identity-guard:" in ci_pr
assert "const skip = verified && singleParent && versionOnly && normalIntent && normal" in ci_pr
assert "versionOnlyRelease" not in ci_pr
assert "Release-Mode" in ci_pr
assert "commit.data.commit.verification" in ci_pr
assert "commit.data.parents" in ci_pr
assert "files[0] === 'VERSION'" in ci_pr
assert "needs: [release-identity-guard]" in ci_pr[ci_pr.index("  unit-tests:"):]
assert "needs.release-identity-guard.outputs.skip != 'true'" in ci_pr
assert "release-identity-guard" in ci_pr
assert "Release Candidate Pipeline" not in release
assert "release_readiness.py" not in release
assert "refs/notes/release" not in release
assert "pull_requests" in text(".github/scripts/release_preparation.py")
assert "covered_product_has_identity" in text(".github/scripts/release_identity.py")
assert "validate_source_boundary" in text(".github/scripts/release_policy.py")
assert "pull_request_changed_files" in text(".github/scripts/release_completion.py")
assert "tag_is_reserved_by_other_pr" in text(".github/scripts/release_completion.py")

quality = json.loads((ROOT / ".github/quality-gates.json").read_text(encoding="utf-8"))
assert quality["required_checks"] == ["Review Policy Gate", "Label Gate", "Release completion"]
assert quality["policy"]["branch_protection"]["require_merge_queue"] is False

lock_start = release.index('          lock_ref_name="release-latest-lock"')
lock_end = release.index('          highest="', lock_start)
lock_body = textwrap.dedent(release[lock_start:lock_end])
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    bin_dir = root / "bin"
    bin_dir.mkdir()
    state_path = root / "lock-state"
    commits_path = root / "commits.json"
    patch_calls_path = root / "patch.calls"
    run_calls_path = root / "run.calls"
    runner_temp = root / "runner-temp"
    runner_temp.mkdir()
    gh_stub = bin_dir / "gh"
    gh_stub.write_text(
        """#!/usr/bin/env python3
import json
import os
import pathlib
import sys

args = sys.argv[1:]
urls = [arg for arg in args if arg.startswith("repos/")]
url = urls[0] if urls else ""
method = args[args.index("--method") + 1] if "--method" in args else "GET"
state_path = pathlib.Path(os.environ["LOCK_STATE"])
commits_path = pathlib.Path(os.environ["LOCK_COMMITS"])
commits = json.loads(commits_path.read_text(encoding="utf-8")) if commits_path.exists() else {}
if url.endswith("/git/ref/heads/release-latest-lock") and method == "GET":
    if not state_path.exists():
        print("HTTP 404: Not Found", file=sys.stderr)
        raise SystemExit(1)
    print(json.dumps({"object": {"sha": state_path.read_text(encoding="utf-8").strip()}}))
    raise SystemExit(0)
if "/actions/runs/" in url:
    run_id = url.rsplit("/", 1)[-1]
    calls_path = pathlib.Path(os.environ["RUN_CALLS"])
    count = int(calls_path.read_text(encoding="utf-8") or "0") if calls_path.exists() else 0
    calls_path.write_text(str(count + 1), encoding="utf-8")
    print("in_progress" if count == 0 else "completed")
    raise SystemExit(0)
if "/git/commits/" in url:
    sha = url.rsplit("/", 1)[-1]
    if "--jq" in args:
        print(commits[sha]["tree"])
    else:
        print(json.dumps({"sha": sha, "tree": {"sha": commits[sha]["tree"]}, "message": commits[sha]["message"]}))
elif url.endswith("/git/commits"):
    message = next(value.split("=", 1)[1] for value in args if value.startswith("message="))
    tree = next(value.split("=", 1)[1] for value in args if value.startswith("tree="))
    parent = next((value.split("=", 1)[1] for value in args if value.startswith("parents[]=")), "")
    sha = f"commit-{len(commits) + 1}"
    commits[sha] = {"message": message, "tree": tree, "parent": parent}
    commits_path.write_text(json.dumps(commits), encoding="utf-8")
    print(sha)
elif url.endswith("/git/refs") and method == "POST":
    if state_path.exists():
        print("HTTP 422: Reference already exists", file=sys.stderr)
        raise SystemExit(1)
    sha = next(value.split("=", 1)[1] for value in args if value.startswith("sha="))
    state_path.write_text(sha, encoding="utf-8")
elif url.endswith("/git/refs/heads/release-latest-lock") and method == "PATCH":
    calls_path = pathlib.Path(os.environ["PATCH_CALLS"])
    count = int(calls_path.read_text(encoding="utf-8") or "0") if calls_path.exists() else 0
    calls_path.write_text(str(count + 1), encoding="utf-8")
    if os.environ.get("FAIL_PATCH") == "1":
        print("HTTP 500: update failed", file=sys.stderr)
        raise SystemExit(1)
    if os.environ.get("CAS_CONFLICT") == "1" and count == 0:
        print("HTTP 409: non-fast-forward", file=sys.stderr)
        raise SystemExit(1)
    sha = next(value.split("=", 1)[1] for value in args if value.startswith("sha="))
    current = state_path.read_text(encoding="utf-8").strip()
    if commits[sha]["parent"] != current:
        print("HTTP 409: non-fast-forward", file=sys.stderr)
        raise SystemExit(1)
    state_path.write_text(sha, encoding="utf-8")
elif "/commits/" in url:
    if "--jq" in args:
        print("tree-sha")
    else:
        print(json.dumps({"commit": {"tree": {"sha": "tree-sha"}}}))
raise SystemExit(0)
""",
        encoding="utf-8",
    )
    gh_stub.chmod(0o755)
    rg_stub = bin_dir / "rg"
    rg_stub.write_text(
        "#!/usr/bin/env bash\n"
        "set -euo pipefail\n"
        "if [[ \"${1:-}\" == \"-q\" ]]; then shift; fi\n"
        "grep -Eq \"$1\" \"$2\"\n",
        encoding="utf-8",
    )
    rg_stub.chmod(0o755)
    sleep_stub = bin_dir / "sleep"
    sleep_stub.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
    sleep_stub.chmod(0o755)
    script = root / "latest-lock.sh"
    script.write_text(
        """#!/usr/bin/env bash
set -euo pipefail
export GITHUB_REPOSITORY=IvanLi-CN/dockrev
export GITHUB_RUN_ID=200
export GITHUB_RUN_ATTEMPT=1
export MERGE_SHA=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
"""
        + lock_body,
        encoding="utf-8",
    )
    script.chmod(0o755)
    env = os.environ.copy()
    env["PATH"] = f"{bin_dir}:{env['PATH']}"
    env["LOCK_STATE"] = str(state_path)
    env["LOCK_COMMITS"] = str(commits_path)
    env["PATCH_CALLS"] = str(patch_calls_path)
    env["RUN_CALLS"] = str(run_calls_path)
    env["RUNNER_TEMP"] = str(runner_temp)
    env["CAS_CONFLICT"] = "1"
    subprocess.run([str(script)], check=True, env=env, cwd=ROOT)
    final_sha = state_path.read_text(encoding="utf-8").strip()
    assert "Release-Latest-Lock-State: released" in json.loads(commits_path.read_text(encoding="utf-8"))[final_sha]["message"]
    commits = json.loads(commits_path.read_text(encoding="utf-8"))
    commits["active"] = {
        "message": """Release latest promotion lock

Release-Latest-Lock-State: acquired
Release-Latest-Lock-Run: 50
Release-Latest-Lock-Attempt: 1
Release-Latest-Lock-Merge-SHA: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa""",
        "tree": "tree-sha",
        "parent": final_sha,
    }
    commits_path.write_text(json.dumps(commits), encoding="utf-8")
    state_path.write_text("active", encoding="utf-8")
    run_calls_path.write_text("0", encoding="utf-8")
    patch_calls_path.write_text("0", encoding="utf-8")
    subprocess.run([str(script)], check=True, env={**env, "CAS_CONFLICT": ""}, cwd=ROOT)
    assert int(run_calls_path.read_text(encoding="utf-8")) >= 2
    state_path.unlink()
    patch_calls_path.write_text("0", encoding="utf-8")
    failed_env = {**env, "CAS_CONFLICT": "", "FAIL_PATCH": "1"}
    failed = subprocess.run([str(script)], check=False, env=failed_env, cwd=ROOT)
    assert failed.returncode != 0

with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    gh_stub = root / "gh"
    gh_stub.write_text(
        """#!/usr/bin/env python3
import json
print(json.dumps([
    {"workflow_runs": [{"event": "push", "conclusion": "success"}]},
    {"workflow_runs": [{"event": "push", "conclusion": "failure"}]},
]))
""",
        encoding="utf-8",
    )
    gh_stub.chmod(0o755)
    recovery_start = release.index('            prior_failure="')
    recovery_end = release.index('            [[ "${prior_failure}"', recovery_start)
    recovery_body = textwrap.dedent(release[recovery_start:recovery_end])
    script = root / "recovery-check.sh"
    script.write_text(
        """#!/usr/bin/env bash
set -euo pipefail
export GITHUB_REPOSITORY=IvanLi-CN/dockrev
export MERGE_SHA=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
"""
        + recovery_body
        + '[[ "${prior_failure}" == "True" ]]\n',
        encoding="utf-8",
    )
    script.chmod(0o755)
    env = os.environ.copy()
    env["PATH"] = f"{root}:{env['PATH']}"
    subprocess.run([str(script)], check=True, env=env, cwd=ROOT)

print("PASS: release workflow contract")
