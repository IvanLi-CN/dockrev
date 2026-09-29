#!/usr/bin/env python3
"""Deterministic contracts for manual version allocation and recovery."""

from __future__ import annotations

import base64
import io
import json
import os
import tempfile
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import check_release_artifact_digest
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

with patch.object(
    release_preparation,
    "graphql",
    return_value={"createCommitOnBranch": {"commit": {"oid": release_identity_sha}}},
) as create_commit:
    created_identity = release_preparation.create_identity_commit(
        "https://api.github.test", "token", "IvanLi-CN/dockrev",
        "release-preparation/v0.81.1", old_main_sha,
        release_policy.compute_target("0.81.0", "patch"),
    )
assert created_identity == release_identity_sha
encoded_version = create_commit.call_args.args[3]["input"]["fileChanges"]["additions"][0]["contents"]
assert base64.b64decode(encoded_version, validate=True) == b"0.81.1\n"

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
         "head_branch": "main", "path": ".github/workflows/release.yml@main",
         "updated_at": "2026-09-20T10:00:00Z"},
        {"id": 101, "event": "workflow_dispatch", "conclusion": "failure", "head_sha": "e" * 40,
         "head_branch": "main", "path": ".github/workflows/release.yml",
         "updated_at": "2026-09-21T10:00:00Z"},
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
failure_context_payload = {
    "repository": "IvanLi-CN/dockrev",
    "merge_commit_sha": "d" * 40,
    "run_attempt": 1,
    "run_url": "https://github.com/IvanLi-CN/dockrev/actions/runs/102",
    "artifact_digest": "f" * 64,
}
failure_context_archive = io.BytesIO()
with zipfile.ZipFile(failure_context_archive, "w", zipfile.ZIP_DEFLATED) as archive:
    archive.writestr("release-failure-context.json", json.dumps(failure_context_payload))
assert release_recovery_artifact.failure_context_digest_from_zip(
    failure_context_archive.getvalue(),
    repository="IvanLi-CN/dockrev",
    merge_sha="d" * 40,
    run_id="102",
    attempt=1,
) == "sha256:" + "f" * 64
failure_context_payload["artifact_digest"] = ""
empty_digest_archive = io.BytesIO()
with zipfile.ZipFile(empty_digest_archive, "w", zipfile.ZIP_DEFLATED) as archive:
    archive.writestr("release-failure-context.json", json.dumps(failure_context_payload))
assert release_recovery_artifact.failure_context_digest_from_zip(
    empty_digest_archive.getvalue(),
    repository="IvanLi-CN/dockrev",
    merge_sha="d" * 40,
    run_id="102",
    attempt=1,
) == ""

redirect_handler = release_recovery_artifact.ArtifactRedirectHandler()
https_request = urllib.request.Request(
    "https://api.github.test/repos/IvanLi-CN/dockrev/actions/artifacts/902/zip",
    headers={"Authorization": "Bearer test-token"},
)
same_origin_redirect = redirect_handler.redirect_request(
    https_request, None, 302, "Found", {},
    "https://api.github.test/repos/IvanLi-CN/dockrev/artifact-redirect",
)
assert same_origin_redirect.get_header("Authorization") == "Bearer test-token"
scheme_downgrade_redirect = redirect_handler.redirect_request(
    https_request, None, 302, "Found", {},
    "http://api.github.test/repos/IvanLi-CN/dockrev/artifact-redirect",
)
assert scheme_downgrade_redirect.get_header("Authorization") is None
cross_host_redirect = redirect_handler.redirect_request(
    https_request, None, 302, "Found", {},
    "https://artifact-store.github.test/download/902",
)
assert cross_host_redirect.get_header("Authorization") is None


class RedirectProbeResponse:
    headers = SimpleNamespace(get_content_charset=lambda: "utf-8")

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def read(self):
        return b"{}"


class RedirectProbeOpener:
    def __init__(self, handler_type):
        self.handler = handler_type()

    def open(self, request, timeout):
        assert timeout == 30
        assert request.get_header("Authorization") == "Bearer test-token"
        redirected = self.handler.redirect_request(
            request, None, 302, "Found", {},
            "http://api.github.test/repos/IvanLi-CN/dockrev/actions/runs/102",
        )
        assert redirected.get_header("Authorization") is None
        return RedirectProbeResponse()


with patch.object(
    urllib.request, "build_opener", side_effect=RedirectProbeOpener,
) as build_api_opener, patch.object(
    urllib.request, "urlopen", side_effect=AssertionError("api_json bypassed redirect-safe opener"),
):
    assert release_recovery_artifact.api_json(
        "https://api.github.test", "test-token", "/repos/IvanLi-CN/dockrev/actions/runs/102",
    ) == {}
    build_api_opener.assert_called_once_with(release_recovery_artifact.ArtifactRedirectHandler)

recovery_merge_sha = "d" * 40
recovery_context_artifact = {
    "id": 902,
    "name": "release-failure-context-102-1",
    "expired": False,
    "workflow_run": {"id": 102},
}
recovery_identity_artifact = {
    "name": "release-intent-" + recovery_merge_sha,
    "expired": False,
    "workflow_run": {"id": 102},
}
recovery_run = {
    "id": 102,
    "path": ".github/workflows/release.yml@main",
    "event": "push",
    "conclusion": "failure",
    "head_sha": recovery_merge_sha,
    "head_branch": "main",
    "run_attempt": 1,
    "updated_at": "2026-09-22T10:00:00Z",
}
recovery_api_artifacts = {
    recovery_identity_artifact["name"]: [recovery_identity_artifact],
    recovery_context_artifact["name"]: [recovery_context_artifact],
    "release-bundle-" + recovery_merge_sha: [],
}

def recovery_api(_api_root, _token, path):
    if "/actions/artifacts?" in path:
        name = urllib.parse.parse_qs(urllib.parse.urlsplit(path).query)["name"][0]
        return {"artifacts": recovery_api_artifacts.get(name, [])}
    if path.endswith("/actions/runs/102"):
        return recovery_run
    raise AssertionError(f"unexpected recovery API path: {path}")

with patch.object(release_recovery_artifact, "api_json", side_effect=recovery_api), \
     patch.object(release_recovery_artifact, "api_bytes", return_value=empty_digest_archive.getvalue()):
    rebuild_from_pre_bundle_failure = release_recovery_artifact.resolve_recovery_artifact(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", recovery_merge_sha,
    )
assert rebuild_from_pre_bundle_failure["artifact_run_id"] == ""
assert rebuild_from_pre_bundle_failure["artifact_digest"] == ""

recovery_run["path"] = ".github/workflows/other.yml"
with patch.object(release_recovery_artifact, "api_json", side_effect=recovery_api):
    expect_error(
        release_recovery_artifact.resolve_recovery_artifact,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", recovery_merge_sha,
        error=release_recovery_artifact.RecoveryArtifactError,
    )
recovery_run["path"] = ".github/workflows/release.yml@main"

recovery_context_artifact["name"] = "release-failure-context-103-1"
with patch.object(release_recovery_artifact, "api_json", side_effect=recovery_api), \
     patch.object(release_recovery_artifact, "api_bytes", return_value=empty_digest_archive.getvalue()):
    expect_error(
        release_recovery_artifact.resolve_recovery_artifact,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", recovery_merge_sha,
        error=release_recovery_artifact.RecoveryArtifactError,
    )
recovery_context_artifact["name"] = "release-failure-context-102-1"

recovery_api_artifacts[recovery_context_artifact["name"]][0]["workflow_run"]["id"] = 103
with patch.object(release_recovery_artifact, "api_json", side_effect=recovery_api):
    expect_error(
        release_recovery_artifact.resolve_recovery_artifact,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", recovery_merge_sha,
        error=release_recovery_artifact.RecoveryArtifactError,
    )
recovery_api_artifacts[recovery_context_artifact["name"]][0]["workflow_run"]["id"] = 102

recovery_api_artifacts["release-bundle-" + recovery_merge_sha] = [{
    "id": 501,
    "name": "release-bundle-" + recovery_merge_sha,
    "expired": False,
    "digest": "sha256:" + "f" * 64,
    "workflow_run": {"id": 102},
}]
with patch.object(release_recovery_artifact, "api_json", side_effect=recovery_api), \
     patch.object(release_recovery_artifact, "api_bytes", return_value=failure_context_archive.getvalue()):
    reused_bundle = release_recovery_artifact.resolve_recovery_artifact(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", recovery_merge_sha,
    )
assert reused_bundle["artifact_run_id"] == "102"
assert reused_bundle["artifact_digest"] == "sha256:" + "f" * 64
mismatched_context_payload = {**failure_context_payload, "artifact_digest": "e" * 64}
mismatched_context_archive = io.BytesIO()
with zipfile.ZipFile(mismatched_context_archive, "w", zipfile.ZIP_DEFLATED) as archive:
    archive.writestr("release-failure-context.json", json.dumps(mismatched_context_payload))
with patch.object(release_recovery_artifact, "api_json", side_effect=recovery_api), \
     patch.object(release_recovery_artifact, "api_bytes", return_value=mismatched_context_archive.getvalue()):
    expect_error(
        release_recovery_artifact.resolve_recovery_artifact,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", recovery_merge_sha,
        error=release_recovery_artifact.RecoveryArtifactError,
    )
recovery_api_artifacts["release-bundle-" + recovery_merge_sha] = []
with patch.object(release_recovery_artifact, "api_json", side_effect=recovery_api), \
     patch.object(release_recovery_artifact, "api_bytes", return_value=failure_context_archive.getvalue()):
    expect_error(
        release_recovery_artifact.resolve_recovery_artifact,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", recovery_merge_sha,
        error=release_recovery_artifact.RecoveryArtifactError,
    )
expect_error(
    release_recovery_artifact.failure_context_digest_from_zip,
    failure_context_archive.getvalue(),
    repository="IvanLi-CN/dockrev",
    merge_sha="e" * 40,
    run_id="102",
    attempt=1,
    error=release_recovery_artifact.RecoveryArtifactError,
)
expect_error(
    release_recovery_artifact.failure_context_digest_from_zip,
    failure_context_archive.getvalue(),
    repository="IvanLi-CN/dockrev",
    merge_sha="d" * 40,
    run_id="102",
    attempt=2,
    error=release_recovery_artifact.RecoveryArtifactError,
)
expect_error(
    release_recovery_artifact.failure_context_digest_from_zip,
    failure_context_archive.getvalue(),
    repository="OtherOwner/dockrev",
    merge_sha="d" * 40,
    run_id="102",
    attempt=1,
    error=release_recovery_artifact.RecoveryArtifactError,
)
expect_error(
    release_recovery_artifact.failure_context_digest_from_zip,
    b"not a zip archive",
    repository="IvanLi-CN/dockrev",
    merge_sha="d" * 40,
    run_id="102",
    attempt=1,
    error=release_recovery_artifact.RecoveryArtifactError,
)
artifact_name = "release-bundle-" + "d" * 40
artifact_metadata = {
    "artifacts": [{
        "name": artifact_name,
        "expired": False,
        "digest": "sha256:" + "f" * 64,
    }],
}
assert check_release_artifact_digest.verify_artifact(
    artifact_metadata,
    artifact_name=artifact_name,
    expected_digest="f" * 64,
)["name"] == artifact_name
assert check_release_artifact_digest.verify_artifact(
    artifact_metadata,
    artifact_name=artifact_name,
    expected_digest="sha256:" + "f" * 64,
)["name"] == artifact_name
expect_error(
    check_release_artifact_digest.verify_artifact,
    artifact_metadata,
    artifact_name=artifact_name,
    expected_digest="e" * 64,
    error=check_release_artifact_digest.ArtifactDigestError,
)
expect_error(
    check_release_artifact_digest.verify_artifact,
    {"artifacts": [{**artifact_metadata["artifacts"][0], "expired": True}]},
    artifact_name=artifact_name,
    expected_digest="f" * 64,
    error=check_release_artifact_digest.ArtifactDigestError,
)
expect_error(
    check_release_artifact_digest.verify_artifact,
    {"artifacts": artifact_metadata["artifacts"] * 2},
    artifact_name=artifact_name,
    expected_digest="f" * 64,
    error=check_release_artifact_digest.ArtifactDigestError,
)
expect_error(
    check_release_artifact_digest.verify_artifact,
    artifact_metadata,
    artifact_name=artifact_name,
    expected_digest="sha256:not-a-digest",
    error=check_release_artifact_digest.ArtifactDigestError,
)
expect_error(
    check_release_artifact_digest.verify_artifact,
    {"artifacts": [{**artifact_metadata["artifacts"][0], "digest": "not-a-digest"}]},
    artifact_name=artifact_name,
    expected_digest="f" * 64,
    error=check_release_artifact_digest.ArtifactDigestError,
)
expect_error(
    check_release_artifact_digest.verify_artifact,
    {"artifacts": [{"name": artifact_name, "expired": False}]},
    artifact_name=artifact_name,
    expected_digest="f" * 64,
    error=check_release_artifact_digest.ArtifactDigestError,
)
without_bundle_runs = [{"id": 102, "event": "push", "conclusion": "failure", "head_sha": "d" * 40,
                        "head_branch": "main", "path": ".github/workflows/release.yml",
                        "run_attempt": 1, "updated_at": "2026-09-22T10:00:00Z"}]
without_bundle_identities = [{"name": "release-intent-" + "d" * 40, "expired": False,
                              "workflow_run": {"id": 102}}]
rebuild_same_identity = release_recovery_artifact.select_reusable_artifact(
    without_bundle_runs,
    without_bundle_identities,
    [],
    "d" * 40,
    failure_context_digests={"102": ""},
)
assert rebuild_same_identity["artifact_run_id"] == ""
assert rebuild_same_identity["artifact_digest"] == ""
expect_error(
    release_recovery_artifact.select_reusable_artifact,
    without_bundle_runs,
    without_bundle_identities,
    [{"id": 502, "name": "release-bundle-" + "d" * 40, "expired": True,
      "digest": "sha256:" + "f" * 64, "workflow_run": {"id": 102}}],
    "d" * 40,
    failure_context_digests={"102": ""},
    error=release_recovery_artifact.RecoveryArtifactError,
)
expect_error(
    release_recovery_artifact.select_reusable_artifact,
    without_bundle_runs,
    without_bundle_identities,
    [],
    "d" * 40,
    failure_context_digests={"102": "sha256:" + "f" * 64},
    error=release_recovery_artifact.RecoveryArtifactError,
)
expect_error(
    release_recovery_artifact.select_reusable_artifact,
    without_bundle_runs,
    without_bundle_identities,
    [],
    "d" * 40,
    failure_context_digests={},
    error=release_recovery_artifact.RecoveryArtifactError,
)

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

raw_digest_context = release_failure_context.resolved_identity_failure_context(
    valid_identity,
    repository="IvanLi-CN/dockrev",
    server="https://github.com",
    run_id="9001",
    attempt="2",
    event="workflow_dispatch",
    ref="refs/heads/main",
    actor="maintainer",
    artifact_digest="f" * 64,
)
with patch.dict(os.environ, {
    "GITHUB_REPOSITORY": "IvanLi-CN/dockrev",
    "GITHUB_SERVER_URL": "https://github.com",
    "EXPECTED_RELEASE_RUN_ID": "9001",
    "EXPECTED_RELEASE_ATTEMPT": "2",
}):
    raw_digest_summary = release_failure_context.notification_summary(raw_digest_context)
assert "artifact digest: " + "f" * 64 in raw_digest_summary

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

same_sha_preflight_context = {
    "merge_commit_sha": "d" * 40,
    "run_url": "https://github.com/IvanLi-CN/dockrev/actions/runs/9003",
    "recovery_instruction": release_policy.SAME_SHA_RECOVERY_PRECHECK_INSTRUCTION,
    "identity_resolution_failed": True,
    "identity_failure_kind": "same-sha-recovery-preflight",
    "repository": "IvanLi-CN/dockrev",
    "workflow": "Release",
    "event": "workflow_dispatch",
    "ref": "refs/heads/main",
    "run_attempt": 1,
    "actor": "maintainer",
}
with patch.dict(os.environ, {
    "GITHUB_REPOSITORY": "IvanLi-CN/dockrev",
    "GITHUB_SERVER_URL": "https://github.com",
    "EXPECTED_RELEASE_RUN_ID": "9003",
    "EXPECTED_RELEASE_ATTEMPT": "1",
}):
    preflight_summary = release_failure_context.notification_summary(same_sha_preflight_context)
assert "failure kind: same-sha-recovery-preflight" in preflight_summary
assert release_policy.SAME_SHA_RECOVERY_PRECHECK_INSTRUCTION in preflight_summary

print("PASS: manual version release identity, reservation, artifact, and recovery contracts")
