#!/usr/bin/env python3
"""Offline fixtures for reusable CI/source-gate validation contracts."""

from __future__ import annotations

import importlib.util
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


scope = load("scope", ROOT / ".github/scripts/resolve-ci-scope.py")
gate = load("gate", ROOT / ".github/scripts/release_source_gate.py")

assert scope.matches("web/src/App.tsx", scope.WEB_PATTERNS)
assert scope.matches("crates/api/src/lib.rs", scope.DOCKER_PATTERNS)
assert not scope.matches("docs/README.md", scope.DOCKER_PATTERNS)
assert scope.resolve("base", "head", True)[:2] == (True, True)

target = "a" * 40
push_attestation = {
    "target_sha": target,
    "gate": "source-build-release",
    "scope": "changed",
    "web": True,
    "docker": True,
    "source_result": "success",
    "publish": False,
    "verification_mode": False,
}
assert gate.validate_push_attestation(push_attestation, target) == (True, "ok")
assert gate.validate_push_attestation(dict(push_attestation, target_sha="b" * 40), target)[0] is False
assert gate.validate_push_attestation(dict(push_attestation, publish=True), target)[0] is False
assert gate.validate_push_attestation(dict(push_attestation, verification_mode=True), target)[0] is False

full_attestation = {
    "target_sha": target,
    "gate": "source-build-release",
    "scope": "full",
    "web": True,
    "docker": True,
    "source_result": "success",
    "publish": False,
}
assert gate.validate_attestation(full_attestation, target) == (True, "ok")

verification = {
    "target_sha": target,
    "fast_target_sha": target,
    "scope": "full",
    "web": True,
    "docker": True,
    "fast_result": "success",
    "source_result": "success",
    "coverage_result": "success",
    "verification_mode": True,
    "publish": False,
    "storybook_shard_total": 3,
    "storybook_story_count": 100,
    "coverage_summary": "c" * 64,
}
assert gate.validate_verification_metrics(verification, target) == (True, "ok")
assert gate.validate_verification_metrics(dict(verification, publish=True), target)[0] is False

print("PASS: CI release gate fixtures")
