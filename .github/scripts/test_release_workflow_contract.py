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
release = text(".github/workflows/release.yml")
notify = text(".github/workflows/notify-release-failure.yml")
ci_pr = text(".github/workflows/ci-pr.yml")

assert "name: Label Gate" in label_gate and "pull_request_target:" in label_gate
assert "workflows: [\"CI (PR)\", \"Label Gate\"]" in preparation
assert "group: release-preparation-${{ inputs.pr_number || github.event.workflow_run.pull_requests[0].number || github.run_id }}" in preparation
assert "name: Publish Release completion check" in preparation
assert "checks.create" in preparation
assert "head_sha: process.env.HEAD_SHA" in preparation
assert "--head-sha \"${HEAD_SHA}\"" in preparation
assert "release-reservation" in text(".github/scripts/release_preparation.py")
assert "checks: write" in preparation
assert "actions: read" in preparation
assert "name: 'Release completion'" in preparation
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
assert "Capture resolved identity step failure context" in release
assert "steps.resolve.outputs.release_enabled == 'true'" in release
assert "resolver-error: retry Release workflow" in release
assert '"identity_failure_kind": "identity-step-failure"' in text(".github/scripts/release_failure_context.py")
assert "steps.upload_identity.outcome == 'failure'" in release
assert "release-publish-stable" in release
assert "format('release-publish-{0}', needs.identity.outputs.merge_sha)" in release
assert "release-latest-lock" in release
assert "Release-Latest-Lock-Run:" in release
assert "matching-refs/heads/release-latest-lock/" in release
assert "HTTP 404|Not Found|404" in release
assert release.index("trap release_lock EXIT") < release.index("while true; do")
assert "git/refs/heads/${lock_ref_name}" in release
assert 'else\n              local status=$?\n            fi' in release
assert "tr '[:upper:]' '[:lower:]'" in release
assert "overwrite: true" in release
assert "github.run_attempt" in release
assert "oidrune/.github/workflows/notify.yml@" in notify
assert "name: release-failure-context-${{ github.event.workflow_run.id }}-${{ github.event.workflow_run.run_attempt }}" in notify
assert "EXPECTED_RELEASE_RUN_ID" in notify
assert "required_secrets" in text(".github/release-failure-notification.json")
assert "release-identity-guard:" in ci_pr
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
assert "pull_request_changed_files" in text(".github/scripts/release_completion.py")
assert "tag_is_reserved_by_other_pr" in text(".github/scripts/release_completion.py")

quality = json.loads((ROOT / ".github/quality-gates.json").read_text(encoding="utf-8"))
assert quality["required_checks"] == ["Review Policy Gate", "Label Gate", "Release completion"]
assert quality["policy"]["branch_protection"]["require_merge_queue"] is False

lock_start = release.index("          lock_suffix=")
lock_end = release.index('          highest="', lock_start)
lock_body = textwrap.dedent(release[lock_start:lock_end])
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    bin_dir = root / "bin"
    bin_dir.mkdir()
    log_path = root / "deletions.log"
    calls_path = root / "matching-refs.calls"
    gh_stub = bin_dir / "gh"
    gh_stub.write_text(
        """#!/usr/bin/env python3
import json
import os
import pathlib
import sys

args = sys.argv[1:]
joined = " ".join(args)
urls = [arg for arg in args if arg.startswith("repos/")]
url = urls[0] if urls else ""
method = args[args.index("--method") + 1] if "--method" in args else "GET"
if method == "DELETE":
    if os.environ.get("FAIL_DELETE") == "1" and url.endswith("release-latest-lock/100-1"):
        print("HTTP 500: cleanup failed", file=sys.stderr)
        raise SystemExit(1)
    with open(os.environ["DELETE_LOG"], "a", encoding="utf-8") as handle:
        handle.write(url + "\\n")
    raise SystemExit(0)
if "matching-refs" in url:
    calls_path = pathlib.Path(os.environ["MATCHING_REFS_CALLS"])
    count = int(calls_path.read_text(encoding="utf-8") or "0") if calls_path.exists() else 0
    calls_path.write_text(str(count + 1), encoding="utf-8")
    refs = [{"ref": "refs/heads/release-latest-lock/200-1"}]
    if count == 0:
        refs.extend(
            [
                {"ref": "refs/heads/release-latest-lock/50-1"},
                {"ref": "refs/heads/release-latest-lock/100-1"},
                {"ref": "refs/heads/release-latest-lock/101-1"},
            ]
        )
    print(json.dumps([refs]))
    raise SystemExit(0)
if "/actions/runs/" in url:
    run_id = url.rsplit("/", 1)[-1]
    if run_id == "50":
        print("in_progress")
    elif run_id == "100":
        print("completed")
    elif run_id == "101":
        print("HTTP 404: Not Found", file=sys.stderr)
        raise SystemExit(1)
    else:
        print("in_progress")
    raise SystemExit(0)
if "git/commits" in url:
    print("lock-sha")
elif "/commits/" in url:
    print("tree-sha")
raise SystemExit(0)
""",
        encoding="utf-8",
    )
    gh_stub.chmod(0o755)
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
    env["DELETE_LOG"] = str(log_path)
    env["MATCHING_REFS_CALLS"] = str(calls_path)
    subprocess.run([str(script)], check=True, env=env, cwd=ROOT)
    deleted = log_path.read_text(encoding="utf-8").splitlines()
    assert any(value.endswith("release-latest-lock/100-1") for value in deleted)
    assert any(value.endswith("release-latest-lock/101-1") for value in deleted)
    assert any(value.endswith("release-latest-lock/200-1") for value in deleted)
    calls_path.write_text("0", encoding="utf-8")
    failed_env = {**env, "FAIL_DELETE": "1"}
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
