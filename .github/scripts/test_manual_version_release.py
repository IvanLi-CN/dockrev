#!/usr/bin/env python3
"""Deterministic contracts for target-only manual release delivery."""

from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch

import release_artifact_bundle
import release_failure_context
import release_image_digests
import release_intent
import release_policy


def expect_error(function, *args, error=ValueError, **kwargs):
    try:
        function(*args, **kwargs)
    except error:
        return
    raise AssertionError(f"{function.__name__} did not raise {error.__name__}")


policy_path = release_policy.POLICY_PATH
release_policy.load_policy()
with tempfile.TemporaryDirectory() as tmp:
    tampered = Path(tmp) / "policy.json"
    tampered.write_text(
        json.dumps({**json.loads(policy_path.read_text(encoding="utf-8")), "source_ci_workflow": "Release"})
    )
    expect_error(release_policy.load_policy, tampered, error=release_policy.PolicyError)

for version_input, expected in (
    ("major", "1.0.0"),
    ("minor", "0.82.0"),
    ("patch", "0.81.1"),
    ("alpha", "0.81.1-alpha.1"),
    ("beta", "0.81.1-beta.1"),
    ("rc", "0.81.1-rc.1"),
):
    decision = release_policy.compute_target("0.81.0", version_input)
    assert decision["version"] == expected, decision
    assert decision["baseline_version"] == "0.81.0"

for tags, version_input, expected in (
    (["v0.81.1-beta.1"], "beta", "0.81.1-beta.2"),
    (["v0.81.1-beta.1"], "rc", "0.81.1-rc.1"),
    (["v0.81.1-rc.1"], "rc", "0.81.1-rc.2"),
    (["v0.81.1-rc.1"], "0.81.1", "0.81.1"),
    (["v0.81.1-alpha.2"], "beta", "0.81.1-beta.1"),
    (["v0.81.12-beta.1"], "rc", "0.81.12-rc.1"),
    (["v0.81.1-rc.1"], "0.81.1-rc.2", "0.81.1-rc.2"),
    (["v0.81.1-beta.1"], "0.81.1", "0.81.1"),
):
    assert release_policy.compute_target("0.81.0", version_input, tags)["version"] == expected

for tags, version_input in (
    ([], "0.81.2"),
    ([], "v0.81.1"),
    ([], "0.81.1+build.1"),
    ([], "dev"),
    ([], ""),
    (["v0.81.1-alpha.1"], "rc"),
    (["v0.81.12-alpha.1"], "rc"),
    (["v0.81.1-rc.1"], "beta"),
    (["v0.81.1-beta.1"], "alpha"),
    (["v0.81.1-rc.1"], "0.81.1-rc.1"),
    (["v0.81.1-rc.2"], "0.81.1-rc.2"),
    (["v0.81.1-rc.1"], "0.81.1-rc.3"),
):
    expect_error(
        release_policy.compute_target, "0.81.0", version_input, tags,
        error=release_policy.PolicyError,
    )

expect_error(
    release_policy.compute_target, "0.81.1-alpha.1", "patch",
    error=release_policy.PolicyError,
)

valid_identity = {
    "source_sha": "c" * 40,
    "baseline_version": "0.81.0",
    "version_input": "rc",
    "version": "0.81.1-rc.1",
    "channel": "rc",
    "release_tag": "v0.81.1-rc.1",
}
image_digests = {"dockrev": "sha256:" + "a" * 64, "dockrev-supervisor": "sha256:" + "b" * 64}

resolved_context = {
    "target": "rc",
    "source_sha": "c" * 40,
    "baseline_version": "0.81.0",
    "version_input": "rc",
    "version": "0.81.1-rc.1",
    "channel": "rc",
    "tag": "v0.81.1-rc.1",
    "artifact_names": ["dockrev_0.81.1-rc.1_linux_amd64_gnu.tar.gz"],
    "artifact_digest": "sha256:" + "d" * 64,
    "image_digests": image_digests,
    "run_url": "https://github.com/IvanLi-CN/dockrev/actions/runs/42",
    "recovery_instruction": "re-run Release from main with target=rc",
    "repository": "IvanLi-CN/dockrev",
    "workflow": "Release",
    "event": "workflow_dispatch",
    "ref": "refs/heads/main",
    "run_attempt": 1,
    "actor": "IvanLi-CN",
}
release_policy.validate_failure_context(resolved_context)
summary = release_failure_context.notification_summary(resolved_context)
assert "version: 0.81.1-rc.1 (rc)" in summary
assert "image digests: dockrev=sha256:" + "a" * 64 in summary
expect_error(
    release_policy.validate_failure_context,
    {**resolved_context, "tag": "v0.81.1"},
    error=release_policy.PolicyError,
)
expect_error(
    release_policy.validate_failure_context,
    {**resolved_context, "recovery_instruction": "retry"},
    error=release_policy.PolicyError,
)

preflight_context = {
    "target": "rc",
    "source_sha": "c" * 40,
    "identity_failure_kind": "preflight",
    "recovery_instruction": release_policy.PREFLIGHT_RECOVERY_INSTRUCTION,
    "run_url": "https://github.com/IvanLi-CN/dockrev/actions/runs/43",
    "repository": "IvanLi-CN/dockrev",
    "workflow": "Release",
    "event": "workflow_dispatch",
    "ref": "refs/heads/main",
    "run_attempt": 1,
    "actor": "IvanLi-CN",
}
release_policy.validate_failure_context(preflight_context)
assert "preflight did not resolve" in release_failure_context.notification_summary(preflight_context)
assert "target: unavailable" in release_failure_context.unresolved_summary(
    repository="IvanLi-CN/dockrev", server="https://github.com", run_id="44",
    attempt="1", event="workflow_dispatch", ref="refs/heads/main", actor="IvanLi-CN",
)

expect_error(
    release_policy.validate_image_digests,
    {"dockrev": "sha256:" + "a" * 64},
    error=release_policy.PolicyError,
)
expect_error(
    release_policy.validate_image_digests,
    {**image_digests, "dockrev": "a" * 64},
    error=release_policy.PolicyError,
)
image_record = release_image_digests.build_payload(valid_identity, image_digests)
assert release_image_digests.validate_payload(image_record, valid_identity)["images"] == image_digests
expect_error(
    release_image_digests.validate_payload,
    {**image_record, "identity": {**image_record["identity"], "version": "0.81.2"}},
    valid_identity,
    error=release_image_digests.ImageDigestError,
)


def published_packages(_api_root, _token, path):
    digest = "a" * 64 if "dockrev-supervisor" not in path else "b" * 64
    return [{"name": "sha256:" + digest, "metadata": {"container": {"tags": ["0.81.1-rc.1"]}}}]


with patch.object(release_image_digests, "api_json", published_packages):
    verified = release_image_digests.verify_remote(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "0.81.1-rc.1", image_digests,
    )
assert verified["images"] == image_digests

with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    assets = root / "assets"
    files = root / "files"
    for arch in ("amd64", "arm64"):
        for libc in ("gnu", "musl"):
            path = assets / arch / libc
            path.mkdir(parents=True)
            for binary in ("dockrev", "dockrev-supervisor"):
                (path / binary).write_bytes(binary.encode())
    files.mkdir()
    for arch in ("amd64", "arm64"):
        for libc in ("gnu", "musl"):
            for binary in ("dockrev", "dockrev-supervisor"):
                base = f"{binary}_{valid_identity['version']}_linux_{arch}_{libc}.tar.gz"
                (files / base).write_bytes(base.encode())
                (files / f"{base}.sha256").write_bytes(b"digest\n")
    bundle = root / "bundle"
    manifest = release_artifact_bundle.create_bundle(assets, files, bundle, valid_identity)
    assert manifest["identity"]["source_sha"] == valid_identity["source_sha"]
    assert release_artifact_bundle.verify_bundle(bundle, valid_identity)["identity"]["version"] == "0.81.1-rc.1"
    expect_error(
        release_artifact_bundle.verify_bundle,
        bundle,
        {**valid_identity, "version": "0.81.2"},
        error=release_artifact_bundle.BundleError,
    )


def intent_api(_api_root, _token, path):
    if path.endswith("/commits/" + "c" * 40):
        return {"commit": {"verification": {"verified": True}}}
    if path.endswith("/git/ref/heads/main"):
        return {"object": {"sha": "e" * 40}}
    if "/compare/" in path:
        return {"status": "ahead"}
    if "/actions/workflows/ci-main.yml/runs" in path:
        return {"workflow_runs": [{"conclusion": "success"}]}
    raise AssertionError(f"unexpected intent request {path}")


with patch.object(release_intent, "api_json", intent_api):
    resolved = release_intent.resolve_intent(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "c" * 40,
        "refs/heads/main", "rc", "ci-main.yml",
    )
assert resolved == {"target": "rc", "source_sha": "c" * 40}
expect_error(
    release_intent.resolve_intent,
    "https://api.github.test", "token", "IvanLi-CN/dockrev", "c" * 40,
    "refs/heads/dev", "rc", "ci-main.yml",
    error=release_intent.IntentError,
)
with patch.object(release_intent, "api_json", lambda *args: {"workflow_runs": [{"conclusion": "failure"}]}):
    expect_error(
        release_intent.ci_success,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "c" * 40, "ci-main.yml",
        error=release_intent.IntentError,
    )
with patch.object(release_intent, "api_json", lambda *args: {"workflow_runs": []}):
    release_intent.baseline_ci_ok(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "c" * 40, "ci-main.yml",
    )
with patch.object(release_intent, "api_json", lambda *args: {"workflow_runs": [{"conclusion": "failure"}]}):
    expect_error(
        release_intent.baseline_ci_ok,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "c" * 40, "ci-main.yml",
        error=release_intent.IntentError,
    )


def baseline_api(_api_root, _token, path):
    if "/releases?per_page=100&page=1" in path:
        return [
            {"tag_name": "v0.81.11-rc.1", "draft": False, "prerelease": True,
             "author": {"login": "github-actions[bot]"}},
            {"tag_name": "v0.81.10", "draft": False, "prerelease": False,
             "author": {"login": "github-actions[bot]"}},
        ]
    if path.endswith("/git/ref/tags/v0.81.10"):
        return {"object": {"type": "commit", "sha": "c" * 40}}
    if "/compare/" in path:
        return {"status": "ahead"}
    if "/actions/workflows/ci-main.yml/runs" in path:
        return {"workflow_runs": []}
    raise AssertionError(f"unexpected baseline request {path}")


with patch.object(release_intent, "api_json", baseline_api):
    baseline = release_intent.resolve_baseline(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "ci-main.yml",
    )
assert baseline == {"version": "0.81.10", "tag": "v0.81.10", "source_sha": "c" * 40}
with patch.object(release_intent, "api_json", lambda *args: []):
    expect_error(
        release_intent.resolve_baseline,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "ci-main.yml",
        error=release_intent.IntentError,
    )

with patch.object(release_intent, "api_json", lambda *args: {"status": "ahead", "commits": [{"sha": "c" * 40}]}):
    assert release_intent.resolve_range(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "b" * 40, "c" * 40,
    )["commits"] == 1
with patch.object(release_intent, "api_json", lambda *args: {"status": "behind", "commits": []}):
    expect_error(
        release_intent.resolve_range,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "b" * 40, "c" * 40,
        error=release_intent.IntentError,
    )

with tempfile.TemporaryDirectory() as tmp:
    tags = Path(tmp) / "tags.txt"
    tags.write_text("v0.81.1-beta.1\nv0.81.1-rc.1\n")
    resolved_version = release_intent.resolve_version("0.81.0", "rc", tags)
assert resolved_version["version"] == "0.81.1-rc.2"
assert resolved_version["release_tag"] == "v0.81.1-rc.2"

print("PASS: target-only manual release delivery contracts")
