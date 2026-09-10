#!/usr/bin/env python3
"""Offline fixtures for exact-SHA readiness, recovery, and FIFO queue semantics."""

from __future__ import annotations

import importlib.util
import json
import tempfile
from argparse import Namespace
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("release_readiness", ROOT / ".github/scripts/release_readiness.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def receipt(target: str, *, publish: bool = False) -> dict:
    return {
        "schema_version": 1,
        "target_sha": target,
        "candidate_run_id": 100,
        "candidate_workflow": "Release Candidate Pipeline",
        "candidate_event": "push",
        "verification_mode": False,
        "publish": publish,
        "source_gate": {
            "target_sha": target,
            "run_id": 101,
            "artifact_name": "source-gate-attestation-101",
            "attestation_sha256": "b" * 64,
            "gate_result": "success",
            "publish": False,
        },
        "preparation": {
            "target_sha": target,
            "run_id": 102,
            "artifact_name": f"release-preparation-{target}",
            "manifest_sha256": "c" * 64,
            "publish": False,
        },
        "created_at": "2026-09-08T00:00:00Z",
    }


def recovery_receipt(target: str) -> dict:
    payload = receipt(target)
    payload.update(
        {
            "candidate_event": "workflow_dispatch",
            "operation": "recover",
            "recovery": {
                "mode": "candidate-recovery",
                "target_sha": target,
                "actor": "release-admin",
                "reason": "restore missing historical candidate",
            },
        }
    )
    return payload


old_target = "a" * 40
new_target = "d" * 40
valid = module.validate_receipt(receipt(old_target), expected_sha=old_target)
assert valid["publish"] is False
recovered = module.validate_receipt(recovery_receipt(old_target), expected_sha=old_target)
assert recovered["operation"] == "recover"
ledger = {
    "schema_version": 1,
    "target_sha": old_target,
    "receipt_ledger": [receipt(old_target), recovery_receipt(old_target)],
}
assert module.validate_receipt(ledger, expected_sha=old_target)["operation"] == "recover"

original_readiness_git = module.git
original_fetch_tags = module.release_snapshot.fetch_tags
original_fetch_notes = module.release_snapshot.fetch_notes_ref
original_read_publication = module.release_snapshot.read_publication
original_read_snapshot = module.release_snapshot.read_snapshot
original_read_override = module.release_snapshot.read_override
original_released = module.release_snapshot.released_commits_from_tags
original_first_parent = module.release_snapshot.first_parent_commits
original_load_pr = module.release_snapshot.load_pr_for_commit
original_labels = module.release_snapshot.current_pr_labels
try:
    module.git = lambda *args, **kwargs: type("Result", (), {"returncode": 0, "stdout": "", "stderr": ""})()
    module.release_snapshot.fetch_tags = lambda: None
    module.release_snapshot.fetch_notes_ref = lambda *_args: None
    module.release_snapshot.read_publication = lambda *_args: None
    module.release_snapshot.read_override = lambda *_args: None
    module.release_snapshot.released_commits_from_tags = lambda _target: set()
    module.release_snapshot.first_parent_commits = lambda _target: [old_target, new_target]
    module.release_snapshot.load_pr_for_commit = lambda *args, **kwargs: {"labels": [{"name": "type:patch"}, {"name": "channel:stable"}]}
    module.release_snapshot.current_pr_labels = lambda _pr: ["type:patch", "channel:stable"]
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / "preflight-output"
        args = Namespace(
            target_sha=old_target,
            repository="IvanLi-CN/dockrev",
            token="token",
            api_root="https://api.github.com",
            main_ref="origin/main",
            publication_notes_ref="refs/notes/release-publications",
            override_notes_ref="refs/notes/release-overrides",
            github_output=str(output),
        )
        assert module.recover_preflight(args) == 0
        assert "release_enabled=true" in output.read_text()
        module.release_snapshot.read_override = lambda _ref, target: {"status": "skip"} if target == old_target else None
        args.target_sha = new_target
        assert module.recover_preflight(args) == 0
        module.release_snapshot.read_override = lambda *_args: None
        module.release_snapshot.released_commits_from_tags = lambda _target: {new_target}
        try:
            module.recover_preflight(args)
        except module.ReadinessError as error:
            assert "already has a release tag" in str(error)
        else:
            raise AssertionError("recovery preflight accepted a tag-only historical target")
        module.release_snapshot.released_commits_from_tags = lambda _target: set()
        args.target_sha = new_target
        try:
            module.recover_preflight(args)
        except module.ReadinessError as error:
            assert "oldest unreleased target" in str(error)
        else:
            raise AssertionError("recovery preflight accepted a newer target")
        module.release_snapshot.first_parent_commits = lambda ref: [old_target] if ref == "origin/main" else [new_target]
        args.target_sha = new_target
        try:
            module.recover_preflight(args)
        except module.ReadinessError as error:
            assert "main first-parent chain" in str(error)
        else:
            raise AssertionError("recovery preflight accepted a side-branch target")
        preanchor = "e" * 40
        trusted_anchor = "f" * 40
        module.release_snapshot.first_parent_commits = lambda _ref: [preanchor, trusted_anchor, new_target]
        module.release_snapshot.released_commits_from_tags = lambda _target: {preanchor, trusted_anchor}
        module.release_snapshot.read_snapshot = lambda *_args: None
        module.release_snapshot.read_publication = lambda _ref, target: {"target_sha": target} if target == trusted_anchor else None
        args.target_sha = new_target
        module.release_snapshot.read_override = lambda _ref, target: {"status": "skip"} if target == preanchor else None
        assert module.recover_preflight(args) == 0
        module.release_snapshot.read_override = lambda *_args: None
        try:
            module.recover_preflight(args)
        except module.ReadinessError as error:
            assert preanchor in str(error)
        else:
            raise AssertionError("recovery preflight bypassed an older untrusted tag")
finally:
    module.git = original_readiness_git
    module.release_snapshot.fetch_tags = original_fetch_tags
    module.release_snapshot.fetch_notes_ref = original_fetch_notes
    module.release_snapshot.read_publication = original_read_publication
    module.release_snapshot.read_snapshot = original_read_snapshot
    module.release_snapshot.read_override = original_read_override
    module.release_snapshot.released_commits_from_tags = original_released
    module.release_snapshot.first_parent_commits = original_first_parent
    module.release_snapshot.load_pr_for_commit = original_load_pr
    module.release_snapshot.current_pr_labels = original_labels

for payload, expected in (
    (receipt(old_target, publish=True), "publish marker"),
    (dict(receipt(old_target), target_sha=new_target), "target_sha mismatch"),
    (dict(receipt(old_target), verification_mode=True), "verification-mode"),
    (dict(receipt(old_target), operation=[]), "operation must be a string"),
    (dict(recovery_receipt(old_target), recovery={"mode": "candidate-recovery", "target_sha": old_target, "actor": "admin", "reason": "  "}), "recovery reason is required"),
    (dict(recovery_receipt(old_target), recovery={"mode": "candidate-recovery", "target_sha": old_target, "actor": "admin"}), "recovery reason is required"),
    (dict(receipt(old_target), preparation=dict(receipt(old_target)["preparation"], artifact_name="wrong")), "artifact"),
):
    try:
        module.validate_receipt(payload, expected_sha=old_target)
    except module.ReadinessError as error:
        assert expected in str(error), error
    else:
        raise AssertionError("invalid readiness receipt was accepted")

original_pending = module.release_snapshot.pending_release_targets
original_fetch = module.fetch_notes
original_snapshot_fetch = module.release_snapshot.fetch_notes_ref
original_tags = module.release_snapshot.fetch_tags
original_git_output = module.git_output
original_git = module.git
original_read = module.read_receipt
try:
    module.git_output = lambda *args: new_target
    module.git = lambda *args, **kwargs: type("Result", (), {"returncode": 0, "stdout": "", "stderr": ""})()
    module.fetch_notes = lambda *_args: None
    module.release_snapshot.fetch_notes_ref = lambda *_args: None
    module.release_snapshot.fetch_tags = lambda: None
    module.release_snapshot.pending_release_targets = lambda *args, **kwargs: [old_target, new_target]
    module.read_receipt = lambda _ref, target: receipt(target) if target == new_target else None
    args = Namespace(
        upper_bound="",
        main_ref="origin/main",
        readiness_notes_ref="refs/notes/release-readiness",
        snapshot_notes_ref="refs/notes/release-snapshots",
        publication_notes_ref="refs/notes/release-publications",
        override_notes_ref="refs/notes/release-overrides",
    )
    assert module.pending_ready_targets(args) == []
    module.read_receipt = lambda _ref, target: receipt(target) if target == old_target else None
    assert module.pending_ready_targets(args) == [old_target]
finally:
    module.release_snapshot.pending_release_targets = original_pending
    module.fetch_notes = original_fetch
    module.release_snapshot.fetch_notes_ref = original_snapshot_fetch
    module.release_snapshot.fetch_tags = original_tags
    module.git_output = original_git_output
    module.git = original_git
    module.read_receipt = original_read

with tempfile.TemporaryDirectory() as directory:
    output = Path(directory) / "output"
    module.export_receipt(receipt(old_target), str(output))
    values = dict(line.split("=", 1) for line in output.read_text().splitlines())
    assert values["readiness_target_sha"] == old_target
    assert values["readiness_publish"] == "false"
    assert values["preparation_manifest_sha256"] == "c" * 64

legacy_note = json.dumps(receipt(old_target))
stored_note = {"value": legacy_note}
original_write_git = module.git
original_write_fetch = module.fetch_notes
try:
    class Result:
        def __init__(self, returncode: int = 0, stdout: str = "", stderr: str = ""):
            self.returncode = returncode
            self.stdout = stdout
            self.stderr = stderr

    def fake_write_git(*args, **kwargs):
        if args[:2] == ("notes", "--ref=refs/notes/release-readiness") and "show" in args:
            return Result(stdout=stored_note["value"])
        if args[:2] == ("notes", "--ref=refs/notes/release-readiness") and "add" in args:
            path = Path(args[args.index("-F") + 1])
            stored_note["value"] = path.read_text()
            return Result()
        if args[:2] == ("push", "origin"):
            return Result()
        return Result(returncode=1)

    module.git = fake_write_git
    module.fetch_notes = lambda _ref: None
    module.write_receipt(recovery_receipt(old_target), "refs/notes/release-readiness", 1)
    ledger_payload = json.loads(stored_note["value"])
    assert len(ledger_payload["receipt_ledger"]) == 2
    before = stored_note["value"]
    module.write_receipt(recovery_receipt(old_target), "refs/notes/release-readiness", 1)
    assert stored_note["value"] == before
finally:
    module.git = original_write_git
    module.fetch_notes = original_write_fetch

print("PASS: release readiness fixtures")
