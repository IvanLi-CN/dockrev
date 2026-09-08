#!/usr/bin/env python3
"""Offline fixtures for the exact-SHA preparation manifest contract."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("release_preparation", ROOT / ".github/scripts/release_preparation.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


target = "a" * 40
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    for relative in module.FIXED_FILES:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(relative.encode())
    route_contract = root / "web/dist/.dockrev-route-contract.json"
    route_contract.parent.mkdir(parents=True, exist_ok=True)
    route_contract.write_text('{"version": 1}\n')
    (root / "web/dist/assets").mkdir(parents=True)
    (root / "web/dist/assets/index.js").write_text("console.log('fixture');\n")

    manifest = module.build_manifest(
        root,
        target_sha=target,
        event="workflow_call",
        workflow_sha="b" * 40,
        workflow_ref="IvanLi-CN/dockrev/.github/workflows/release-candidate.yml@refs/heads/main",
    )
    valid, reason = module.validate_manifest(manifest, target)
    assert valid, reason
    assert manifest["publish"] is False
    assert manifest["verification_mode"] is False
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

    for payload, reason_text in (
        (dict(manifest, target_sha="c" * 40), "target_sha"),
        (dict(manifest, publish=True), "publish"),
        (dict(manifest, event="schedule"), "event"),
        (dict(manifest, workflow_file="other.yml"), "source"),
        (dict(manifest, workflow_sha="invalid"), "workflow_sha"),
        (dict(manifest, workflow_ref="refs/heads/feature"), "workflow_ref"),
        (dict(manifest, verification_mode="false"), "verification_mode"),
    ):
        valid, reason = module.validate_manifest(payload, target)
        assert not valid and reason_text in reason, (payload, reason)

    manual = module.build_manifest(
        root,
        target_sha=target,
        event="workflow_dispatch",
        workflow_sha="b" * 40,
        workflow_ref="IvanLi-CN/dockrev/.github/workflows/release-preparation.yml@refs/heads/main",
        verification_mode=True,
    )
    valid, reason = module.validate_manifest(manual, target, allow_verification=True)
    assert valid, reason

print("PASS: release preparation fixtures")
