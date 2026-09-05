#!/usr/bin/env python3
"""Unit fixtures for the release preparation manifest and recovery contract."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
from argparse import Namespace
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("release_preparation", ROOT / ".github/scripts/release_preparation.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def expect_invalid(payload, target, text):
    valid, reason = module.validate_manifest(payload, target, allow_recovery=True)
    assert not valid, payload
    assert text in reason, reason


def expect_preparation_error(action, text):
    try:
        action()
    except module.PreparationError as error:
        assert text in str(error), error
    else:
        raise AssertionError("expected PreparationError")


def exercise_recovery_replacement(manifest):
    legacy_run = {
        "id": 101,
        "status": "completed",
        "conclusion": "failure",
        "head_sha": "c" * 40,
    }
    recovered_run = {
        "id": 102,
        "status": "completed",
        "conclusion": "success",
        "head_sha": "d" * 40,
    }
    dispatched = []
    original_api_request = module.api_request
    original_recovery_runs = module.recovery_runs
    original_workflow_runs = module.workflow_runs
    original_successful_manifest = module.successful_manifest
    original_sleep = module.time.sleep
    try:
        def fake_api_request(api_root, token, path, *, method="GET", body=None):
            if path.endswith(f"/compare/{module.ARTIFACT_UPLOAD_CONTRACT_SHA}...{'c' * 40}"):
                return {"status": "behind"}
            if path.endswith("/dispatches"):
                dispatched.append((method, body))
                return None
            raise AssertionError(path)

        def fake_recovery_runs(api_root, repository, token, recovery_request):
            fake_recovery_runs.calls += 1
            if fake_recovery_runs.calls < 3:
                return [legacy_run]
            return [legacy_run, recovered_run]

        fake_recovery_runs.calls = 0

        def fake_workflow_runs(api_root, repository, token, target_sha):
            return []

        def fake_successful_manifest(api_root, repository, token, runs, target_sha):
            if recovered_run in runs:
                return recovered_run, manifest
            return None

        def fake_sleep(seconds):
            fake_sleep.calls.append(seconds)

        fake_sleep.calls = []

        module.api_request = fake_api_request
        module.recovery_runs = fake_recovery_runs
        module.workflow_runs = fake_workflow_runs
        module.successful_manifest = fake_successful_manifest
        module.time.sleep = fake_sleep
        result = module.ensure_preparation(
            Namespace(
                api_root="https://api.example.test",
                repository="owner/repository",
                token="token",
                target_sha=target,
                timeout_seconds=60,
                poll_seconds=1,
            )
        )
    finally:
        module.api_request = original_api_request
        module.recovery_runs = original_recovery_runs
        module.workflow_runs = original_workflow_runs
        module.successful_manifest = original_successful_manifest
        module.time.sleep = original_sleep

    assert dispatched == [
        (
            "POST",
            {
                "ref": "main",
                "inputs": {"target_sha": target, "recovery_request": f"release-recovery-{target}"},
            },
        )
    ], dispatched
    assert fake_sleep.calls == [1], fake_sleep.calls
    assert result["run_id"] == recovered_run["id"], result
    assert result["recovery"] is True, result


def exercise_recovery_failure_is_exhausted(contract_status):
    current_run = {
        "id": 201,
        "status": "completed",
        "conclusion": "failure",
        "head_sha": "e" * 40,
    }
    dispatched = []
    original_api_request = module.api_request
    original_recovery_runs = module.recovery_runs
    original_wait_for_manifest = module.wait_for_manifest
    try:
        def fake_api_request(api_root, token, path, *, method="GET", body=None):
            if path.endswith(f"/compare/{module.ARTIFACT_UPLOAD_CONTRACT_SHA}...{'e' * 40}"):
                return {"status": contract_status}
            if path.endswith("/dispatches"):
                dispatched.append((method, body))
                return None
            raise AssertionError(path)

        def fake_recovery_runs(api_root, repository, token, recovery_request):
            return [current_run]

        def fake_wait_for_manifest(*args, **kwargs):
            return None

        module.api_request = fake_api_request
        module.recovery_runs = fake_recovery_runs
        module.wait_for_manifest = fake_wait_for_manifest
        expect_preparation_error(
            lambda: module.ensure_preparation(
                Namespace(
                    api_root="https://api.example.test",
                    repository="owner/repository",
                    token="token",
                    target_sha=target,
                    timeout_seconds=60,
                    poll_seconds=1,
                )
            ),
            "single recovery" if contract_status == "ahead" else "linear ancestry",
        )
    finally:
        module.api_request = original_api_request
        module.recovery_runs = original_recovery_runs
        module.wait_for_manifest = original_wait_for_manifest

    assert not dispatched, dispatched


target = "a" * 40
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    for relative in module.FIXED_FILES:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(relative.encode())
    web_file = root / "web/dist/assets/index.js"
    web_file.parent.mkdir(parents=True, exist_ok=True)
    web_file.write_text("console.log('fixture');\n")
    route_contract = root / "web/dist/.dockrev-route-contract.json"
    route_contract.write_text('{"version": 1}\n')

    manifest = module.build_manifest(
        root,
        target_sha=target,
        event="push",
        workflow_sha="b" * 40,
        workflow_ref="IvanLi-CN/dockrev/.github/workflows/release-preparation.yml@refs/heads/main",
        recovery_request="",
    )
    valid, reason = module.validate_manifest(manifest, target)
    assert valid, reason
    assert manifest["publish"] is False
    assert manifest["head_branch"] == "main"
    assert len(manifest["files"]) == len(module.FIXED_FILES) + 2
    assert module.manifest_digest(manifest) == module.manifest_digest(json.loads(json.dumps(manifest)))
    manifest_sha256 = module.manifest_digest(manifest)
    valid, reason = module.verify_manifest_files(root, manifest, target, expected_manifest_sha256=manifest_sha256)
    assert valid, reason

    manifest_path = root / "release-preparation-manifest.json"
    manifest_path.write_text(json.dumps(manifest))
    verify_command = [
        sys.executable,
        str(ROOT / ".github/scripts/release_preparation.py"),
        "verify",
        "--root",
        str(root),
        "--manifest",
        str(manifest_path),
        "--target-sha",
        target,
        "--expected-manifest-sha256",
        manifest_sha256,
    ]
    verified = subprocess.run(verify_command, cwd=ROOT, capture_output=True, text=True, check=False)
    assert verified.returncode == 0, verified.stderr
    assert verified.stdout.strip() == "ok", verified.stdout

    manifest_path.write_text(json.dumps(dict(manifest, created_at="tampered")))
    verified = subprocess.run(verify_command, cwd=ROOT, capture_output=True, text=True, check=False)
    assert verified.returncode == 1, verified.stdout
    assert "digest" in verified.stdout, verified.stdout
    manifest_path.write_text(json.dumps(manifest))

    missing_route_contract = dict(
        manifest,
        files=[entry for entry in manifest["files"] if entry["path"] != "web/dist/.dockrev-route-contract.json"],
    )
    expect_invalid(missing_route_contract, target, "route contract")

    route_contract.unlink()
    valid, reason = module.verify_manifest_files(root, manifest, target, expected_manifest_sha256=manifest_sha256)
    assert not valid and "missing" in reason, reason
    verified = subprocess.run(verify_command, cwd=ROOT, capture_output=True, text=True, check=False)
    assert verified.returncode == 1, verified.stdout
    assert "missing" in verified.stdout, verified.stdout

    try:
        module.build_manifest(
            root,
            target_sha=target,
            event="push",
            workflow_sha="b" * 40,
            workflow_ref="IvanLi-CN/dockrev/.github/workflows/release-preparation.yml@refs/heads/main",
            recovery_request="",
        )
    except module.PreparationError as error:
        assert "web/dist/.dockrev-route-contract.json" in str(error), error
    else:
        raise AssertionError("manifest generation accepted a missing route contract")

    route_contract.symlink_to(web_file)
    valid, reason = module.verify_manifest_files(root, manifest, target)
    assert not valid and "unsafe" in reason, reason
    route_contract.unlink()
    route_contract.write_text('{"version": 1}\n')

    wrong_size = dict(manifest, files=[dict(entry) for entry in manifest["files"]])
    wrong_size["files"][-1]["size"] += 1
    valid, reason = module.verify_manifest_files(root, wrong_size, target)
    assert not valid and "size" in reason, reason

    wrong_digest = dict(manifest, files=[dict(entry) for entry in manifest["files"]])
    wrong_digest["files"][-1]["sha256"] = "0" * 64
    valid, reason = module.verify_manifest_files(root, wrong_digest, target)
    assert not valid and "digest" in reason, reason

    expect_invalid(dict(manifest, target_sha="c" * 40), target, "target_sha")
    expect_invalid(dict(manifest, publish=True), target, "publish")
    expect_invalid(dict(manifest, event="schedule"), target, "event")
    expect_invalid(dict(manifest, workflow_file="other.yml"), target, "source")
    expect_invalid(dict(manifest, workflow_sha="invalid"), target, "workflow_sha")
    expect_invalid(dict(manifest, workflow_ref="refs/heads/feature"), target, "workflow_ref")
    expect_invalid(dict(manifest, files=[entry for entry in manifest["files"] if not entry["path"].startswith("web/dist/")]), target, "web/dist")

    recovery = dict(manifest, event="workflow_dispatch", recovery_request=f"release-recovery-{target}")
    valid, reason = module.validate_manifest(recovery, target, allow_recovery=True)
    assert valid, reason
    valid, reason = module.validate_manifest(recovery, target, allow_recovery=False)
    assert not valid and "recovery" in reason
    expect_invalid(dict(recovery, recovery_request="release-recovery-" + "d" * 40), target, "recovery")

    malformed = dict(manifest, files=[{"path": "../escape", "size": 1, "sha256": "0" * 64}])
    expect_invalid(malformed, target, "unsafe")

    exercise_recovery_replacement(manifest)
    exercise_recovery_failure_is_exhausted("ahead")
    exercise_recovery_failure_is_exhausted("diverged")

print("PASS: release preparation fixtures")
