#!/usr/bin/env python3
"""Focused regression fixtures for Dockrev's PR label release contract."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
from argparse import Namespace
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


policy = load("release_policy", ROOT / ".github/scripts/release_policy.py")
completion = load("release_completion", ROOT / ".github/scripts/release_completion.py")
identity = load("release_identity", ROOT / ".github/scripts/release_identity.py")
preparation_script = load("release_preparation", ROOT / ".github/scripts/release_preparation.py")


def expect_error(function, *args, **kwargs):
    try:
        function(*args, **kwargs)
    except (policy.PolicyError, completion.CompletionError, identity.IdentityError, preparation_script.PreparationError):
        return
    raise AssertionError(f"expected {function.__name__} to fail")


labels = policy.parse_labels(["type:patch", "channel:stable", "component:app"])
assert labels["release_enabled"] is True
assert policy.next_patch("0.1.0") == "0.1.1"
assert policy.next_patch("1.4.9") == "1.4.10"
policy.validate_channel_version("0.1.1", "stable")
policy.validate_channel_version("0.2.0-beta.1", "beta")
policy.validate_channel_version("0.2.0-dev.3", "dev")
policy.validate_preparation_version("0.1.0", "0.1.1-beta.1", {"type": "patch", "channel": "beta"})
expect_error(policy.parse_labels, ["type:patch", "type:minor", "channel:stable"])
expect_error(policy.parse_labels, ["type:patch", "channel:rc"])
expect_error(policy.next_patch, "0.1.0-beta.1")
expect_error(policy.validate_channel_version, "0.1.1", "beta")
expect_error(policy.validate_channel_version, "0.1.1-beta.preview", "beta")
expect_error(policy.validate_preparation_version, "0.1.0", "0.1.2-beta.1", {"type": "patch", "channel": "beta"})

source_sha = "a" * 40
prep_sha = "b" * 40
preparation = {
    "commit_sha": prep_sha,
    "source_sha": source_sha,
    "version": "0.1.1",
    "intent": policy.parse_labels(["type:patch", "channel:stable"]),
    "source_version": "0.1.0",
    "release_intent": "type:patch channel:stable",
    "release_mode": "normal-preparation",
    "parents": [source_sha],
    "changed_files": ["VERSION"],
    "verified": True,
}
assert policy.validate_preparation(preparation, source_sha=source_sha) == preparation
expect_error(policy.validate_preparation, {**preparation, "changed_files": ["src/lib.rs"]})
expect_error(policy.validate_preparation, {**preparation, "verified": False})
expect_error(policy.parse_trailers, "Release-Mode: normal-preparation\nRelease-Mode: normal-preparation")
assert preparation_script.is_existing_preparation({"Release-Mode": "normal-preparation", "Source-SHA": source_sha if 'source_sha' in globals() else "a" * 40, "Product-Version": "0.1.1"})
assert not preparation_script.is_existing_preparation({"Release-Mode": "normal-preparation"})
beta_labels = policy.parse_labels(["type:patch", "channel:beta"])
assert preparation_script.expected_version(beta_labels, "0.1.0", "0.1.1-beta.1") == "0.1.1-beta.1"
expect_error(preparation_script.expected_version, beta_labels, "0.1.0", None)

assert completion.validate_completion({"labels": ["type:none", "channel:stable"]}) == {
    "status": "pass",
    "release_enabled": False,
    "mode": "non-product",
}

original_pull_request = preparation_script.pull_request
original_preparation_api_request = preparation_script.api_request
original_source_ci_ready = preparation_script.source_ci_ready
original_current_version = preparation_script.current_version
original_reserve_tag = preparation_script.reserve_tag
original_create_commit = preparation_script.create_commit
original_inspect_commit = preparation_script.inspect_commit
try:
    calls = {"source_ci": 0, "reserve": 0, "create": 0}

    def fake_pull_request(_api_root, _token, _repository, _number):
        return {
            "state": "open",
            "base": {"ref": "main"},
            "head": {"sha": source_sha, "ref": "feature/release", "repo": {"full_name": "IvanLi-CN/dockrev"}},
            "labels": [{"name": "type:patch"}, {"name": "channel:stable"}],
        }

    def fake_preparation_api_request(_api_root, _token, _method, path, _payload=None):
        assert path.endswith(f"/commits/{source_sha}")
        return {"commit": {"message": "Product change"}}

    def fake_source_ci(*_args):
        calls["source_ci"] += 1

    def fake_current_version(*_args):
        return "0.1.0"

    def fake_reserve(*_args):
        calls["reserve"] += 1

    def fake_create_commit(*_args):
        calls["create"] += 1
        return prep_sha

    def fake_inspect(*_args):
        return preparation

    preparation_script.pull_request = fake_pull_request
    preparation_script.api_request = fake_preparation_api_request
    preparation_script.source_ci_ready = fake_source_ci
    preparation_script.current_version = fake_current_version
    preparation_script.reserve_tag = fake_reserve
    preparation_script.create_commit = fake_create_commit
    preparation_script.inspect_commit = fake_inspect
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / "release-intent.json"
        preparation_script.create(Namespace(
            api_root="https://api.github.test",
            token="token",
            repository="IvanLi-CN/dockrev",
            pr_number=42,
            exact_version=None,
            output=output,
        ))
        result = json.loads(output.read_text(encoding="utf-8"))
        assert result["release_enabled"] is True
        assert result["version"] == "0.1.1"
        assert calls == {"source_ci": 1, "reserve": 1, "create": 1}

        calls = {"source_ci": 0, "reserve": 0, "create": 0}
        preparation_script.pull_request = lambda *_args: {
            "state": "open",
            "base": {"ref": "main"},
            "head": {"sha": source_sha, "ref": "feature/release", "repo": {"full_name": "IvanLi-CN/dockrev"}},
            "labels": [{"name": "type:none"}, {"name": "channel:stable"}],
        }
        output = Path(directory) / "non-product.json"
        preparation_script.create(Namespace(
            api_root="https://api.github.test",
            token="token",
            repository="IvanLi-CN/dockrev",
            pr_number=42,
            exact_version=None,
            output=output,
        ))
        result = json.loads(output.read_text(encoding="utf-8"))
        assert result["release_enabled"] is False
        assert calls == {"source_ci": 0, "reserve": 0, "create": 0}
finally:
    preparation_script.pull_request = original_pull_request
    preparation_script.api_request = original_preparation_api_request
    preparation_script.source_ci_ready = original_source_ci_ready
    preparation_script.current_version = original_current_version
    preparation_script.reserve_tag = original_reserve_tag
    preparation_script.create_commit = original_create_commit
    preparation_script.inspect_commit = original_inspect_commit

captured = {}
original_graphql = preparation_script.graphql
def fake_graphql(_api_root, _token, _query, variables):
    captured["variables"] = variables
    return {"createCommitOnBranch": {"commit": {"oid": "d" * 40}}}

preparation_script.graphql = fake_graphql
assert preparation_script.create_commit(
    "https://api.github.test", "token", "IvanLi-CN/dockrev", "feature/release", source_sha, "0.1.1", labels
) == "d" * 40
preparation_script.graphql = original_graphql
commit_input = captured["variables"]["input"]
assert commit_input["expectedHeadOid"] == source_sha
assert commit_input["fileChanges"]["additions"][0]["path"] == "VERSION"
assert "Release-Mode: normal-preparation" in commit_input["message"]["body"]

original_api_request = preparation_script.api_request
preparation_script.api_request = lambda _api_root, _token, _method, path, _payload=None: (
    {
        f"/repos/IvanLi-CN/dockrev/commits/{prep_sha}": {
            "parents": [{"sha": source_sha}],
            "files": [{"filename": "VERSION"}],
            "commit": {"verification": {"verified": True}, "message": "Release-Mode: normal-preparation\nSource-SHA: " + source_sha + "\nProduct-Version: 0.1.1\nRelease-Intent: type:patch channel:stable"},
        },
        "/repos/IvanLi-CN/dockrev/git/ref/heads/feature%2Frelease": {"object": {"sha": prep_sha}},
    }[path]
)
assert preparation_script.inspect_commit(
    "https://api.github.test", "token", "IvanLi-CN/dockrev", "feature/release", prep_sha, source_sha, "0.1.1", labels
)["changed_files"] == ["VERSION"]
preparation_script.api_request = lambda _api_root, _token, _method, path, _payload=None: (
    {"object": {"sha": "e" * 40}} if "git/ref/heads" in path else {
        "parents": [{"sha": source_sha}],
        "files": [{"filename": "VERSION"}],
        "commit": {"verification": {"verified": True}, "message": "Release-Mode: normal-preparation\nSource-SHA: " + source_sha + "\nProduct-Version: 0.1.1\nRelease-Intent: type:patch channel:stable"},
    }
)
expect_error(preparation_script.inspect_commit, "https://api.github.test", "token", "IvanLi-CN/dockrev", "feature/release", prep_sha, source_sha, "0.1.1", labels)
preparation_script.api_request = original_api_request

source_checks = {
    "ci_pr": {"status": "completed", "conclusion": "success"},
    "label_gate": {"status": "completed", "conclusion": "success"},
}
completion_payload = {
    "labels": ["type:patch", "channel:stable", "component:app"],
    "source_sha": source_sha,
    "head_sha": prep_sha,
    "version_file": "0.1.1",
    "base_ref": "main",
    "release_mode": "normal-preparation",
    "preparation": preparation,
    "source_checks": source_checks,
    "tag_reserved": True,
}
assert completion.validate_completion(completion_payload)["status"] == "pass"
expect_error(completion.validate_completion, {**completion_payload, "source_checks": {"ci_pr": source_checks["ci_pr"], "label_gate": {}}})
expect_error(completion.validate_completion, {**completion_payload, "tag_reserved": False})
expect_error(
    completion.validate_completion,
    {
        **completion_payload,
        "preparation": {**preparation, "version": "0.1.2"},
    },
)

original_completion_api_json = completion.api_json
try:
    tag_exists = False
    labels_for_loader = ["type:patch", "channel:stable"]
    preparation_message = (
        "Prepare release identity\n\n"
        f"Source-SHA: {source_sha}\n"
        "Product-Version: 0.1.1\n"
        "Release-Intent: type:patch channel:stable\n"
        "Release-Mode: normal-preparation"
    )

    def fake_completion_api(_api_root, _token, path):
        if path.endswith("/pulls/42"):
            return {
                "state": "open",
                "base": {"ref": "main"},
                "head": {"sha": prep_sha},
                "labels": [{"name": label} for label in labels_for_loader],
            }
        if path.endswith(f"/commits/{prep_sha}"):
            return {
                "parents": [{"sha": source_sha}],
                "files": [{"filename": "VERSION"}] if labels_for_loader[0] != "type:none" else [],
                "commit": {"verification": {"verified": True}, "message": preparation_message if labels_for_loader[0] != "type:none" else "Product change"},
            }
        if path.endswith("/commits/" + source_sha):
            return {"parents": [], "files": [], "commit": {"message": "Product change"}}
        if "/contents/VERSION?ref=" in path:
            value = "0.1.0" if source_sha in path else "0.1.1"
            encoded = __import__("base64").b64encode(value.encode()).decode()
            return {"encoding": "base64", "content": encoded}
        if path.endswith("/actions/workflows/ci-pr.yml/runs?per_page=100"):
            return {"workflow_runs": [{"head_sha": source_sha, "status": "completed", "conclusion": "success", "pull_requests": [{"number": 42}]}]}
        if path.endswith("/actions/workflows/label-gate.yml/runs?per_page=100"):
            return {"workflow_runs": [{"head_sha": source_sha, "status": "completed", "conclusion": "success", "pull_requests": [{"number": 42, "head": {"sha": source_sha}}]}]}
        if path.endswith("/git/ref/tags/v0.1.1"):
            if tag_exists:
                return {"object": {"sha": prep_sha, "type": "commit"}}
            raise completion.CompletionError("GitHub API failed: 404")
        if "/pulls?state=" in path:
            return []
        raise AssertionError(f"unexpected completion API path: {path}")

    completion.api_json = fake_completion_api
    loaded = completion.load_github_completion("https://api.github.test", "token", "IvanLi-CN/dockrev", 42)
    assert completion.validate_completion(loaded)["status"] == "pass"
    tag_exists = True
    expect_error(completion.validate_completion, completion.load_github_completion("https://api.github.test", "token", "IvanLi-CN/dockrev", 42))
    labels_for_loader = ["type:none", "channel:stable"]
    assert completion.load_github_completion("https://api.github.test", "token", "IvanLi-CN/dockrev", 42) == {"labels": labels_for_loader}
finally:
    completion.api_json = original_completion_api_json

covered_sha = "c" * 40
version_only = {
    "labels": ["type:patch", "channel:stable"],
    "source_sha": covered_sha,
    "head_sha": prep_sha,
    "version_file": "0.1.1",
    "base_ref": "main",
    "release_mode": "version-only-release-pr",
    "changed_files": ["VERSION"],
    "provenance": {
        "covered_product_merge_sha": covered_sha,
        "product_version": "0.1.1",
        "release_intent": "type:patch channel:stable",
        "release_mode": "version-only-release-pr",
        "branch_head_sha": prep_sha,
        "covered_product_pr_number": 41,
        "covered_product_head_sha": covered_sha,
        "covered_product_merged": True,
        "covered_product_has_identity": False,
    "verified": True,
    },
    "source_checks": source_checks,
    "tag_reserved": True,
}
assert completion.validate_completion(version_only)["status"] == "pass"
expect_error(completion.validate_completion, {**version_only, "changed_files": []})
expect_error(completion.validate_completion, {**version_only, "provenance": {**version_only["provenance"], "covered_product_merge_sha": "bad"}})
expect_error(
    completion.validate_completion,
    {
        **version_only,
        "labels": ["type:none", "channel:stable"],
        "provenance": {**version_only["provenance"], "release_intent": "type:none channel:stable"},
    },
)

identity_payload = {
    "pull_request": 42,
    "source_sha": source_sha,
    "merge_commit_sha": prep_sha,
    "release_mode": "normal-preparation",
    "version": "0.1.1",
    "version_file": "0.1.1",
    "intent": labels,
    "artifact_names": ["dockrev_0.1.1_linux_amd64_gnu.tar.gz", "dockrev_0.1.1_linux_amd64_gnu.tar.gz.sha256"],
    "run_url": "https://github.com/IvanLi-CN/dockrev/actions/runs/1",
}
resolved = identity.resolve_from_payload(identity_payload)
assert resolved["release_tag"] == "v0.1.1"
expect_error(identity.resolve_from_payload, {**identity_payload, "version": "0.1.1-beta.1"})

failure = {
    "pull_request": 42,
    "source_sha": source_sha,
    "merge_commit_sha": prep_sha,
    "type": "patch",
    "channel": "stable",
    "version": "0.1.1",
    "tag": "v0.1.1",
    "artifact_names": ["dockrev_0.1.1_linux_amd64_gnu.tar.gz", "dockrev_0.1.1_linux_amd64_gnu.tar.gz.sha256"],
    "run_url": "https://github.com/IvanLi-CN/dockrev/actions/runs/1",
    "recovery_instruction": "workflow_dispatch merge_sha=" + prep_sha + " recovery_reason=<required>",
}
assert policy.validate_failure_context(failure) == failure
expect_error(policy.validate_failure_context, {**failure, "tag": "v0.1.0"})
expect_error(policy.validate_failure_context, {**failure, "artifact_names": []})
expect_error(policy.validate_failure_context, {**failure, "run_url": ""})
expect_error(policy.validate_failure_context, failure, expected_repository="IvanLi-CN/dockrev", expected_run_id="2")
expect_error(policy.validate_failure_context, failure, expected_repository="IvanLi-CN/dockrev", expected_run_id="1", expected_server="github.com", expected_attempt="2")
expect_error(policy.validate_failure_context, {**failure, "identity_failure_kind": "unknown"})
expect_error(policy.validate_failure_context, {**failure, "identity_failure_kind": "no-identity"})
identity_failure = {
    **failure,
    "source_sha": prep_sha,
    "version": "0.0.0",
    "tag": "v0.0.0",
    "identity_resolution_failed": True,
    "recovery_instruction": "create VERSION-only release PR Covered-Product-Merge-SHA=" + prep_sha,
}
assert policy.validate_failure_context(identity_failure) == identity_failure
expect_error(policy.validate_failure_context, {**identity_failure, "recovery_instruction": "workflow_dispatch merge_sha=" + prep_sha + " recovery_reason=<required>"})
failure_context = load("release_failure_context", ROOT / ".github/scripts/release_failure_context.py")
assert "recovery:" in failure_context.notification_summary(failure)

with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    files = root / "files.json"
    provenance = root / "provenance.json"
    files.write_text(json.dumps(["VERSION"]))
    provenance.write_text(json.dumps(version_only["provenance"]))
    assert policy.main.__name__ == "main"

print("PASS: PR label release fixtures")
