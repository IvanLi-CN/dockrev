#!/usr/bin/env python3
"""Deterministic contracts for manual version allocation and recovery."""

from __future__ import annotations

import json
import os
import tempfile
import urllib.parse
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import release_artifact_bundle
import release_completion
import release_failure_context
import release_policy
import release_preparation
import release_recovery_artifact


def expect_error(function, *args, error=ValueError, **kwargs):
    try:
        function(*args, **kwargs)
    except error:
        return
    raise AssertionError(f"expected {error.__name__} from {function.__name__}")


release_policy.load_policy()
with tempfile.TemporaryDirectory() as directory:
    policy_path = Path(directory) / "policy.json"
    policy = json.loads(release_policy.POLICY_PATH.read_text(encoding="utf-8"))
    policy["prerelease_sequence_start"] = 2
    policy_path.write_text(json.dumps(policy), encoding="utf-8")
    expect_error(release_policy.load_policy, policy_path, error=release_policy.PolicyError)

for version_input, target, channel in (
    ("major", "1.0.0", "stable"),
    ("minor", "0.82.0", "stable"),
    ("patch", "0.81.1", "stable"),
    ("alpha", "0.81.1-alpha.1", "alpha"),
    ("beta", "0.81.1-beta.1", "beta"),
    ("0.81.1-alpha.1", "0.81.1-alpha.1", "alpha"),
    ("0.81.1-beta.1", "0.81.1-beta.1", "beta"),
):
    actual = release_policy.compute_target("0.81.0", version_input)
    assert (actual["version"], actual["channel"]) == (target, channel), actual

for baseline, version_input, expected in (
    ("0.81.1-alpha.1", "alpha", "0.81.1-alpha.2"),
    ("0.81.1-alpha.4", "beta", "0.81.1-beta.1"),
    ("0.81.1-beta.2", "beta", "0.81.1-beta.3"),
    ("0.81.1-beta.2", "rc", "0.81.1-rc.1"),
    ("0.81.1-rc.2", "rc", "0.81.1-rc.3"),
    ("0.81.1-rc.2", "0.81.1", "0.81.1"),
    ("0.81.1-beta.2", "major", "1.0.0"),
    ("0.81.1-beta.2", "minor", "0.82.0"),
    ("0.81.1-beta.2", "patch", "0.81.2"),
    ("0.81.1-beta.2", "0.81.2", "0.81.2"),
    ("0.81.1-alpha.2", "major", "1.0.0"),
    ("0.81.1-alpha.2", "minor", "0.82.0"),
    ("0.81.1-alpha.2", "patch", "0.81.2"),
    ("0.81.1-rc.2", "major", "1.0.0"),
    ("0.81.1-rc.2", "minor", "0.82.0"),
    ("0.81.1-rc.2", "patch", "0.81.2"),
):
    assert release_policy.compute_target(baseline, version_input)["version"] == expected

for baseline, version_input in (
    ("0.81.0", "rc"),
    ("0.81.1-alpha.1", "rc"),
    ("0.81.1-beta.1", "alpha"),
    ("0.81.1-rc.1", "beta"),
    ("0.81.1-beta.1", "0.81.1"),
    ("0.81.0", "0.81.2"),
    ("0.81.0", "v0.81.1"),
    ("0.81.0", "0.81.1+build.1"),
    ("0.81.0", "0.81.1-dev.1"),
    ("0.81.0", "dev"),
    ("0.81.0", ""),
):
    expect_error(release_policy.compute_target, baseline, version_input, error=release_policy.PolicyError)

assert release_policy.compute_target("0.81.0", "alpha")["version"] == "0.81.1-alpha.1"
assert release_policy.compute_target("0.81.0", "beta")["version"] == "0.81.1-beta.1"

expect_error(release_policy.parse_version, "01.2.3", error=release_policy.PolicyError)
expect_error(release_policy.parse_version, "1.2.3-alpha.01", error=release_policy.PolicyError)
expect_error(release_policy.parse_version, "1.2.3-dev.1", error=release_policy.PolicyError)
assert release_policy.parse_version_file("0.81.0\n") == "0.81.0"
for invalid_contents in (" 0.81.0\n", "0.81.0 \n", "0.81.0\n0.81.1\n", "0.81.0\r\n"):
    expect_error(release_policy.parse_version_file, invalid_contents, error=release_policy.PolicyError)

assert release_policy.compute_target("0.81.0", "0.82.0") == {
    "baseline_version": "0.81.0",
    "version_input": "0.82.0",
    "version": "0.82.0",
    "channel": "stable",
}

with patch.object(release_preparation, "api_request", return_value=None):
    expect_error(
        release_preparation.version_at_ref,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "main",
        error=release_preparation.PreparationError,
    )

old_main_sha = "c" * 40
current_main_sha = "e" * 40
release_identity_sha = "f" * 40
identity_message = """Prepare Dockrev version identity

Source-SHA: cccccccccccccccccccccccccccccccccccccccc
Product-Version: 0.81.1
Release-Baseline-Version: 0.81.0
Release-Intent: patch
"""

def identity_api(_api_root, _token, _method, path, _payload=None):
    if path.endswith(f"/commits/{release_identity_sha}"):
        return {
            "commit": {"verification": {"verified": True}, "message": identity_message},
            "parents": [{"sha": old_main_sha}],
            "files": [{"filename": "VERSION"}],
        }
    if path.endswith(f"VERSION?ref={urllib.parse.quote(release_identity_sha, safe='')}"):
        return {"encoding": "base64", "content": "MC44MS4xCg=="}
    if path.endswith(f"VERSION?ref={urllib.parse.quote(old_main_sha, safe='')}"):
        return {"encoding": "base64", "content": "MC44MS4wCg=="}
    if path.endswith(f"/compare/{old_main_sha}...{current_main_sha}"):
        return {"status": "ahead"}
    raise AssertionError(f"unexpected release preparation API request: {path}")

with patch.object(release_preparation, "api_request", side_effect=identity_api):
    retained = release_preparation.inspect_identity(
        "https://api.github.test", "token", "IvanLi-CN/dockrev",
        release_identity_sha, current_main_sha, "0.81.0",
        release_policy.compute_target("0.81.0", "patch"),
    )
    assert retained["parent_sha"] == old_main_sha

prepare_args = SimpleNamespace(
    api_root="https://api.github.test",
    token="token",
    repository="IvanLi-CN/dockrev",
    version="patch",
)
with (
    patch.object(
        release_preparation,
        "api_request",
        return_value={"object": {"sha": current_main_sha}},
    ),
    patch.object(release_preparation, "version_at_ref", return_value="0.81.0"),
    patch.object(release_preparation, "open_release_preparation_pull_requests", return_value=[]),
    patch.object(release_preparation, "branch_ref", return_value=release_identity_sha),
    patch.object(
        release_preparation,
        "inspect_identity",
        return_value={"identity_sha": release_identity_sha, "parent_sha": old_main_sha},
    ),
    patch.object(release_preparation, "reserve_version") as reserve,
    patch.object(
        release_preparation,
        "find_or_create_pull_request",
        return_value={"number": 77, "html_url": "https://github.test/pull/77"},
    ),
    patch.object(release_preparation, "create_identity_commit") as create_identity,
):
    duplicate = release_preparation.prepare(prepare_args)
assert duplicate["identity_sha"] == release_identity_sha
assert duplicate["baseline_sha"] == old_main_sha
assert duplicate["main_sha"] == current_main_sha
create_identity.assert_not_called()
reserve.assert_called_once_with(
    "https://api.github.test", "token", "IvanLi-CN/dockrev", "0.81.1", release_identity_sha
)

with patch.object(
    release_preparation,
    "open_release_preparation_pull_requests",
    return_value=[{
        "base": {"ref": "main"},
        "head": {
            "ref": "release-preparation/v0.82.0",
            "repo": {"full_name": "IvanLi-CN/dockrev"},
        },
    }],
):
    expect_error(
        release_preparation.assert_no_competing_release_preparation,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "release-preparation/v0.81.1",
        error=release_preparation.PreparationError,
    )

with patch.object(
    release_preparation,
    "open_release_preparation_pull_requests",
    return_value=[{
        "base": {"ref": "main"},
        "head": {
            "ref": "release-preparation/v0.81.1",
            "repo": {"full_name": "fork-owner/dockrev"},
        },
    }],
):
    expect_error(
        release_preparation.assert_no_competing_release_preparation,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "release-preparation/v0.81.1",
        error=release_preparation.PreparationError,
    )

next_main_sha = "d" * 40
with (
    patch.object(
        release_preparation,
        "api_request",
        side_effect=[
            {"object": {"sha": current_main_sha}},
            {"object": {"sha": next_main_sha}},
        ],
    ),
    patch.object(release_preparation, "version_at_ref", side_effect=["0.81.0", "0.81.1"]),
    patch.object(release_preparation, "open_release_preparation_pull_requests", return_value=[]),
    patch.object(release_preparation, "branch_ref", return_value=release_identity_sha),
    patch.object(
        release_preparation,
        "inspect_identity",
        return_value={"identity_sha": release_identity_sha, "parent_sha": old_main_sha},
    ),
    patch.object(release_preparation, "reserve_version") as reserve_after_baseline_move,
    patch.object(release_preparation, "find_or_create_pull_request") as create_pull_after_baseline_move,
    patch.object(release_preparation, "create_identity_commit"),
):
    expect_error(
        release_preparation.prepare,
        prepare_args,
        error=release_preparation.PreparationError,
    )
reserve_after_baseline_move.assert_not_called()
create_pull_after_baseline_move.assert_not_called()

with (
    patch.object(
        release_preparation,
        "api_request",
        side_effect=[
            {"object": {"sha": current_main_sha}},
            {"object": {"sha": current_main_sha}},
            {"object": {"sha": next_main_sha}},
        ],
    ),
    patch.object(release_preparation, "version_at_ref", side_effect=["0.81.0", "0.81.0", "0.81.1"]),
    patch.object(release_preparation, "open_release_preparation_pull_requests", return_value=[]),
    patch.object(release_preparation, "branch_ref", return_value=release_identity_sha),
    patch.object(
        release_preparation,
        "inspect_identity",
        return_value={"identity_sha": release_identity_sha, "parent_sha": old_main_sha},
    ),
    patch.object(release_preparation, "reserve_version") as reserved_during_race,
    patch.object(release_preparation, "find_or_create_pull_request") as pull_during_race,
    patch.object(release_preparation, "create_identity_commit"),
):
    expect_error(
        release_preparation.prepare,
        prepare_args,
        error=release_preparation.PreparationError,
    )
reserved_during_race.assert_called_once()
pull_during_race.assert_not_called()

with (
    patch.object(
        release_preparation,
        "api_request",
        side_effect=[
            {"object": {"sha": current_main_sha}},
            {"object": {"sha": current_main_sha}},
            {"object": {"sha": current_main_sha}},
            {"object": {"sha": next_main_sha}},
        ],
    ),
    patch.object(
        release_preparation,
        "version_at_ref",
        side_effect=["0.81.0", "0.81.0", "0.81.0", "0.81.1"],
    ),
    patch.object(release_preparation, "open_release_preparation_pull_requests", return_value=[]),
    patch.object(release_preparation, "branch_ref", return_value=release_identity_sha),
    patch.object(
        release_preparation,
        "inspect_identity",
        return_value={"identity_sha": release_identity_sha, "parent_sha": old_main_sha},
    ),
    patch.object(release_preparation, "reserve_version"),
    patch.object(
        release_preparation,
        "find_or_create_pull_request",
        return_value={"number": 42},
    ),
    patch.object(release_preparation, "close_stale_pull_request") as close_stale_pr,
    patch.object(release_preparation, "create_identity_commit"),
):
    expect_error(
        release_preparation.prepare,
        prepare_args,
        error=release_preparation.StaleBaselineError,
    )
close_stale_pr.assert_called_once_with(
    prepare_args.api_root,
    prepare_args.token,
    prepare_args.repository,
    {"number": 42},
)

identity_sha = "a" * 40
foreign_sha = "b" * 40
reserved_sha = identity_sha
with patch.object(release_preparation, "branch_ref", side_effect=lambda *_args: reserved_sha):
    release_preparation.reserve_version(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "0.81.1", identity_sha
    )
    reserved_sha = foreign_sha
    expect_error(
        release_preparation.reserve_version,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "0.81.1", identity_sha,
        error=release_preparation.PreparationError,
    )

valid_completion = {
    "repository": "IvanLi-CN/dockrev",
    "pull_request": 42,
    "head_sha": identity_sha,
    "identity_sha": identity_sha,
    "source_sha": "c" * 40,
    "baseline_version": "0.81.0",
    "version_input": "patch",
    "version": "0.81.1",
    "channel": "stable",
    "verified": True,
    "parents": ["c" * 40],
    "changed_files": ["VERSION"],
    "main_version": "0.81.0",
    "reservation_sha": identity_sha,
}
assert release_completion.validate_completion(valid_completion) == valid_completion

completion_source_sha = "c" * 40
completion_head_sha = "a" * 40
completion_pull = {
    "state": "open",
    "base": {"ref": "main", "repo": {"full_name": "IvanLi-CN/dockrev"}},
    "head": {
        "repo": {"full_name": "IvanLi-CN/dockrev"},
        "ref": "release-preparation/v0.81.1",
        "sha": completion_head_sha,
    },
}

def completion_api(_api_root, _token, path):
    if path.endswith("/pulls/42"):
        return completion_pull
    if path.endswith(f"/commits/{completion_head_sha}"):
        return {
            "commit": {
                "verification": {"verified": True},
                "message": identity_message,
            },
            "parents": [{"sha": completion_source_sha}],
            "files": [{"filename": "VERSION"}],
        }
    if path.endswith("/pulls/42/files?per_page=100&page=1"):
        return [{"filename": "VERSION"}]
    if path.endswith(f"/contents/VERSION?ref={completion_source_sha}"):
        return {"encoding": "base64", "content": "MC44MS4wCg=="}
    if path.endswith(f"/contents/VERSION?ref={completion_head_sha}"):
        return {"encoding": "base64", "content": "MC44MS4xCg=="}
    if path.endswith("/contents/VERSION?ref=main"):
        return {"encoding": "base64", "content": "MC44MS4wCg=="}
    if path.endswith(f"/compare/{completion_source_sha}...main"):
        return {"status": "ahead"}
    if path.endswith("/git/ref/heads/release-reservation/v0.81.1"):
        return {"object": {"sha": completion_head_sha}}
    if path.endswith("/git/ref/heads/release-publication-lock/v0.81.1"):
        return None
    if path.endswith("/git/ref/tags/v0.81.1") or path.endswith("/git/ref/tags/0.81.1"):
        return None
    raise AssertionError(f"unexpected release completion API request: {path}")

with patch.object(release_completion, "api_json", side_effect=completion_api):
    loaded_completion = release_completion.load_github_completion(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", 42, completion_head_sha
    )
assert loaded_completion["version"] == "0.81.1"
assert loaded_completion["reservation_sha"] == completion_head_sha

with patch.object(
    release_completion,
    "api_json",
    return_value={
        **completion_pull,
        "head": {
            **completion_pull["head"],
            "repo": {"full_name": "fork-owner/dockrev"},
        },
    },
):
    expect_error(
        release_completion.load_github_completion,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", 42, completion_head_sha,
        error=release_completion.CompletionError,
    )

ordinary_completion = release_completion.validate_nonrelease_completion({
    "repository": "IvanLi-CN/dockrev",
    "pull_request": 7,
    "head_sha": identity_sha,
    "changed_files": ["README.md"],
    "trailers": {},
})
assert ordinary_completion["release_enabled"] is False
expect_error(
    release_completion.validate_nonrelease_completion,
    {"repository": "IvanLi-CN/dockrev", "pull_request": 7, "head_sha": identity_sha,
     "changed_files": ["VERSION"], "trailers": {}},
    error=release_completion.CompletionError,
)
expect_error(
    release_completion.validate_nonrelease_completion,
    {"repository": "IvanLi-CN/dockrev", "pull_request": 7, "head_sha": identity_sha,
     "changed_files": ["README.md"], "trailers": {"Product-Version": "0.81.1"}},
    error=release_completion.CompletionError,
)
expect_error(
    release_completion.validate_completion,
    {**valid_completion, "reservation_sha": foreign_sha},
    error=release_completion.CompletionError,
)
expect_error(
    release_completion.validate_completion,
    {**valid_completion, "main_version": "0.81.1"},
    error=release_completion.CompletionError,
)

valid_identity = {
    "release_enabled": True,
    "pull_request": 42,
    "source_sha": "c" * 40,
    "merge_commit_sha": "d" * 40,
    "identity_sha": identity_sha,
    "version": "0.81.1-beta.1",
    "baseline_version": "0.81.0",
    "version_input": "beta",
    "channel": "beta",
    "release_tag": "v0.81.1-beta.1",
}
release_policy.validate_identity(valid_identity)
expect_error(
    release_policy.validate_identity,
    {**valid_identity, "version": "0.81.1-rc.1"},
    error=release_policy.PolicyError,
)

recovery = release_recovery_artifact.select_reusable_artifact(
    [
        {"id": 100, "event": "push", "conclusion": "failure", "head_sha": "d" * 40,
         "head_branch": "main", "updated_at": "2026-09-20T10:00:00Z"},
        {"id": 101, "event": "workflow_dispatch", "conclusion": "failure", "head_sha": "e" * 40,
         "head_branch": "main", "updated_at": "2026-09-21T10:00:00Z"},
    ],
    [
        {"name": "release-intent-" + "d" * 40, "expired": False,
         "workflow_run": {"id": 100}},
        {"name": "release-intent-" + "d" * 40, "expired": False,
         "workflow_run": {"id": 101}},
    ],
    [
        {"id": 501, "name": "release-bundle-" + "d" * 40, "expired": False,
         "digest": "sha256:" + "f" * 64, "workflow_run": {"id": 100}},
    ],
    "d" * 40,
)
assert recovery["artifact_run_id"] == "100"
assert recovery["artifact_digest"] == "sha256:" + "f" * 64
without_bundle = release_recovery_artifact.select_reusable_artifact(
    [{"id": 102, "event": "push", "conclusion": "failure", "head_sha": "d" * 40,
      "head_branch": "main", "updated_at": "2026-09-22T10:00:00Z"}],
    [{"name": "release-intent-" + "d" * 40, "expired": False,
      "workflow_run": {"id": 102}}],
    [],
    "d" * 40,
)
assert without_bundle["artifact_run_id"] == ""
assert without_bundle["artifact_digest"] == ""

with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    binaries = root / "release-assets"
    packages = root / "dist" / "release"
    for arch in ("amd64", "arm64"):
        for libc in ("gnu", "musl"):
            for binary in ("dockrev", "dockrev-supervisor"):
                path = binaries / arch / libc / binary
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(f"{binary}-{arch}-{libc}".encode())
                package = packages / f"{binary}_0.81.1_linux_{arch}_{libc}.tar.gz"
                package.parent.mkdir(parents=True, exist_ok=True)
                package.write_bytes(f"package-{binary}-{arch}-{libc}".encode())
                package.with_suffix(package.suffix + ".sha256").write_text("fixture\n", encoding="utf-8")
    bundle = root / "bundle"
    nested_manifest = binaries / "amd64" / "gnu" / "manifest.json"
    nested_manifest.write_text("untracked source manifest\n", encoding="utf-8")
    expect_error(
        release_artifact_bundle.create_bundle,
        binaries,
        packages,
        root / "rejected-bundle",
        {
            "source_sha": "c" * 40,
            "merge_commit_sha": "d" * 40,
            "identity_sha": identity_sha,
            "baseline_version": "0.81.0",
            "version_input": "patch",
            "version": "0.81.1",
            "channel": "stable",
            "release_tag": "v0.81.1",
        },
        error=release_artifact_bundle.BundleError,
    )
    nested_manifest.unlink()
    manifest = release_artifact_bundle.create_bundle(binaries, packages, bundle, {
        "source_sha": "c" * 40,
        "merge_commit_sha": "d" * 40,
        "identity_sha": identity_sha,
        "baseline_version": "0.81.0",
        "version_input": "patch",
        "version": "0.81.1",
        "channel": "stable",
        "release_tag": "v0.81.1",
    })
    assert manifest["content_digest"] == release_artifact_bundle.verify_bundle(bundle, {
        "source_sha": "c" * 40,
        "merge_commit_sha": "d" * 40,
        "identity_sha": identity_sha,
        "baseline_version": "0.81.0",
        "version_input": "patch",
        "version": "0.81.1",
        "channel": "stable",
        "release_tag": "v0.81.1",
    })["content_digest"]
    nested_bundle_manifest = bundle / "release-assets/amd64/gnu/manifest.json"
    nested_bundle_manifest.write_text("unlisted nested manifest\n", encoding="utf-8")
    expect_error(
        release_artifact_bundle.verify_bundle,
        bundle,
        {
            "source_sha": "c" * 40,
            "merge_commit_sha": "d" * 40,
            "identity_sha": identity_sha,
            "baseline_version": "0.81.0",
            "version_input": "patch",
            "version": "0.81.1",
            "channel": "stable",
            "release_tag": "v0.81.1",
        },
        error=release_artifact_bundle.BundleError,
    )
    nested_bundle_manifest.unlink()
    (bundle / "release-assets/amd64/gnu/dockrev").write_bytes(b"changed")
    expect_error(
        release_artifact_bundle.verify_bundle,
        bundle,
        {
            "source_sha": "c" * 40,
            "merge_commit_sha": "d" * 40,
            "identity_sha": identity_sha,
            "baseline_version": "0.81.0",
            "version_input": "patch",
            "version": "0.81.1",
            "channel": "stable",
            "release_tag": "v0.81.1",
        },
        error=release_artifact_bundle.BundleError,
    )

failure_context = release_failure_context.resolved_identity_failure_context(
    valid_identity,
    repository="IvanLi-CN/dockrev",
    server="https://github.com",
    run_id="9001",
    attempt="2",
    event="workflow_dispatch",
    ref="refs/heads/main",
    actor="maintainer",
    artifact_digest="sha256:" + "f" * 64,
)
with patch.dict(os.environ, {
    "GITHUB_REPOSITORY": "IvanLi-CN/dockrev",
    "GITHUB_SERVER_URL": "https://github.com",
    "EXPECTED_RELEASE_RUN_ID": "9001",
    "EXPECTED_RELEASE_ATTEMPT": "2",
}):
    summary = release_failure_context.notification_summary(failure_context)
assert "version decision: beta" in summary
assert "artifact digest: sha256:" + "f" * 64 in summary
assert "workflow_dispatch merge_sha=" + "d" * 40 in summary

unavailable_context = release_failure_context.unavailable_identity_failure_context(
    repository="IvanLi-CN/dockrev",
    server="https://github.com",
    run_id="9002",
    attempt="3",
    event="push",
    ref="main",
    actor="github-actions[bot]",
)
with patch.dict(os.environ, {
    "GITHUB_REPOSITORY": "IvanLi-CN/dockrev",
    "GITHUB_SERVER_URL": "https://github.com",
    "EXPECTED_RELEASE_RUN_ID": "9002",
    "EXPECTED_RELEASE_ATTEMPT": "3",
}):
    unavailable_summary = release_failure_context.notification_summary(unavailable_context)
assert "identity could not be resolved" in unavailable_summary
assert "merge sha: unavailable (identity not verified)" in unavailable_summary
assert "workflow_dispatch merge_sha=" not in unavailable_summary

print("PASS: manual version release identity, reservation, artifact, and recovery contracts")
