#!/usr/bin/env python3
"""Static and behavioral checks for generic workflow failure notifications."""

from __future__ import annotations

import importlib.util
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONFIG = ROOT / ".github/release-failure-notification.json"
WORKFLOW = ROOT / ".github/workflows/notify-failed-workflow.yml"
RELEASE_WORKFLOW = ROOT / ".github/workflows/notify-release-failure.yml"
CONTEXT = ROOT / ".github/scripts/workflow_failure_context.py"


config = json.loads(CONFIG.read_text(encoding="utf-8"))
expected = set(config["expected_success_workflows"])
excluded = set(config["excluded_workflows"])
routes = config["failure_notification_routes"]
assert expected.isdisjoint(excluded)
assert routes["Release"] == "Notify failed release"
assert routes["default"] == "Notify failed workflow"


workflow_names = set()
for path in (ROOT / ".github/workflows").glob("*.yml"):
    match = re.search(r"^name:\s*(.+?)\s*$", path.read_text(encoding="utf-8"), re.MULTILINE)
    assert match, f"workflow is missing a top-level name: {path}"
    workflow_names.add(match.group(1).strip().strip('"'))

assert expected | excluded == workflow_names
assert expected <= workflow_names


generic = WORKFLOW.read_text(encoding="utf-8")
assert "name: Notify failed workflow" in generic
assert "types: [completed]" in generic
assert "github.event.workflow_run.conclusion == 'failure'" in generic
assert "ref: ${{ github.event.repository.default_branch }}" in generic
assert "ref: ${{ github.event.workflow_run.head_sha" not in generic
assert "oidrune/.github/workflows/notify.yml@" in generic
assert "python3 .github/scripts/workflow_failure_context.py" in generic

for workflow_name in sorted(expected - {"Release"}):
    assert f'      - "{workflow_name}"' in generic, workflow_name
assert '      - "Release"' not in generic

release_notify = RELEASE_WORKFLOW.read_text(encoding="utf-8")
assert "workflows: [Release]" in release_notify
assert 'name: release-failure-context-${{ github.event.workflow_run.id }}-${{ github.event.workflow_run.run_attempt }}' in release_notify


spec = importlib.util.spec_from_file_location("workflow_failure_context", CONTEXT)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
summary = module.render_summary(
    {
        "repository": "IvanLi-CN/dockrev",
        "server": "https://github.com",
        "workflow": "Docs Pages",
        "event": "push",
        "ref": "main",
        "branch": "main",
        "head_sha": "a" * 40,
        "pull_request": "none",
        "run_id": "4050",
        "run_attempt": "2",
        "actor": "github-actions[bot]",
        "conclusion": "failure",
    }
)
assert "workflow: Docs Pages" in summary
assert "attempt: 2" in summary
assert "run: https://github.com/IvanLi-CN/dockrev/actions/runs/4050" in summary
assert "recovery: inspect failed jobs" in summary

print("PASS: generic workflow failure notification contract")
