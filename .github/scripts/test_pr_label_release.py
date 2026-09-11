#!/usr/bin/env python3
"""Focused regression fixtures for Dockrev's PR label release contract."""

from __future__ import annotations

import importlib.util
import json
import subprocess
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
policy.validate_channel_version("0.2.0-rc.1", "rc")
policy.validate_channel_version("0.2.0-dev.3", "dev")
assert policy.patch_channel_transitions() == {
    "stable": {"stable", "beta", "dev"},
    "beta": {"beta", "rc"},
    "rc": {"rc", "stable"},
    "dev": {"dev"},
}
policy.validate_preparation_version("0.1.0", "0.1.1-beta.1", {"type": "patch", "channel": "beta"})
policy.validate_preparation_version("0.1.1-beta.1", "0.1.1-rc.1", {"type": "patch", "channel": "rc"})
policy.validate_preparation_version("0.1.1-rc.1", "0.1.1", {"type": "patch", "channel": "stable"})
expect_error(policy.parse_labels, ["type:patch", "type:minor", "channel:stable"])
assert policy.parse_labels(["type:patch", "channel:rc"])["channel"] == "rc"
expect_error(policy.parse_labels, ["type:patch", "channel:preview"])
expect_error(policy.validate_source_boundary, [".github/workflows/ci-pr.yml"])
expect_error(policy.validate_source_boundary, [".github/quality-gates.json"])
expect_error(policy.validate_source_boundary, [".github/scripts/check-live-quality-gates.py"])
expect_error(policy.next_patch, "0.1.0-beta.1")
expect_error(policy.validate_channel_version, "0.1.1", "beta")
expect_error(policy.validate_channel_version, "0.1.1-beta.preview", "beta")
expect_error(policy.validate_preparation_version, "0.1.0", "0.1.2-beta.1", {"type": "patch", "channel": "beta"})
expect_error(policy.validate_preparation_version, "0.1.1-beta.1", "0.1.1", {"type": "patch", "channel": "stable"})
expect_error(policy.validate_preparation_version, "0.1.1-rc.1", "0.1.1-beta.2", {"type": "patch", "channel": "beta"})
expect_error(policy.validate_preparation_version, "0.1.1-dev.1", "0.1.1-rc.1", {"type": "patch", "channel": "rc"})
expect_error(policy.validate_preparation_version, "0.1.1-beta.1", "0.1.1-beta.1", {"type": "patch", "channel": "beta"})
expect_error(policy.validate_preparation_version, "0.1.1-rc.2", "0.1.1-rc.1", {"type": "patch", "channel": "rc"})

source_sha = "a" * 40
prep_sha = "b" * 40
preparation = {
    "commit_sha": prep_sha,
    "source_sha": source_sha,
    "source_pr_updated_at": "2026-01-01T00:00:00Z",
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
assert preparation_script.is_version_only_release({"Release-Mode": "version-only-release-pr", "Covered-Product-Merge-SHA": source_sha, "Product-Version": "0.1.1", "Release-Intent": "type:patch channel:stable"})
beta_labels = policy.parse_labels(["type:patch", "channel:beta"])
assert preparation_script.expected_version(beta_labels, "0.1.0", "0.1.1-beta.1") == "0.1.1-beta.1"
expect_error(preparation_script.expected_version, beta_labels, "0.1.0", None)
expect_error(preparation_script.expected_version, beta_labels, "0.1.0", "9.9.9-beta.1")
rc_labels = policy.parse_labels(["type:patch", "channel:rc"])
assert preparation_script.expected_version(rc_labels, "0.1.1-beta.1", "0.1.1-rc.1") == "0.1.1-rc.1"
assert preparation_script.expected_version(rc_labels, "0.1.1-beta.1", " 0.1.1-rc.1 ") == "0.1.1-rc.1"
expect_error(preparation_script.expected_version, rc_labels, "0.1.1-beta.1", None)
expect_error(preparation_script.expected_version, rc_labels, "0.1.1-beta.1", " \t ")
stable_labels = policy.parse_labels(["type:patch", "channel:stable"])
assert preparation_script.expected_version(stable_labels, "0.1.1-rc.1", "0.1.1") == "0.1.1"
expect_error(preparation_script.expected_version, stable_labels, "0.1.1-rc.1", None)

assert completion.validate_completion({"labels": ["type:none", "channel:stable"]}) == {
    "status": "pass",
    "release_enabled": False,
    "mode": "non-product",
}
expect_error(completion.validate_completion, {"labels": ["type:none", "channel:stable"], "changed_files": ["VERSION"]})
assert completion.validate_completion({
    "labels": ["type:none", "channel:stable"],
    "changed_files": ["VERSION"],
    "version_bootstrap": True,
    "base_version_exists": False,
    "version_file": "0.1.0",
})["mode"] == "non-product"
assert completion.validate_completion({
    "labels": ["type:none", "channel:stable"],
    "changed_files": ["VERSION", "docs/release.md"],
    "version_bootstrap": True,
    "base_version_exists": False,
    "version_file": "0.1.0",
})["mode"] == "non-product"
expect_error(completion.validate_completion, {
    "labels": ["type:none", "channel:stable"],
    "changed_files": ["VERSION"],
    "version_bootstrap": True,
    "base_version_exists": True,
    "version_file": "0.1.0",
})

original_preparation_version_api_request = preparation_script.api_request
try:
    preparation_script.api_request = lambda *_args, **_kwargs: {
        "encoding": "base64",
        "content": "MC4xLjAK\n",
    }
    assert preparation_script.current_version(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", source_sha
    ) == "0.1.0"
finally:
    preparation_script.api_request = original_preparation_version_api_request

original_pull_request = preparation_script.pull_request
original_preparation_api_request = preparation_script.api_request
original_source_ci_ready = preparation_script.source_ci_ready
original_current_version = preparation_script.current_version
original_reserve_tag = preparation_script.reserve_tag
original_delete_reservation_ref = preparation_script.delete_reservation_ref
original_reserve_recovery_identity = preparation_script.reserve_recovery_identity
original_reserve_preparation_identity = preparation_script.reserve_preparation_identity
original_create_commit = preparation_script.create_commit
original_inspect_commit = preparation_script.inspect_commit
try:
    calls = {"source_ci": 0, "reserve": 0, "create": 0}
    reserved_sources = []
    reservation_kwargs = []
    reservation_sequence = []
    recovery_identity_reservations = []

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

    def fake_source_ci(*_args, **_kwargs):
        calls["source_ci"] += 1
        return "2026-01-01T00:00:00Z"

    def fake_current_version(*_args):
        return "0.1.0"

    def fake_reserve(*_args, **_kwargs):
        reservation_sequence.append("version")
        calls["reserve"] += 1
        reserved_sources.append(_args[-1])
        reservation_kwargs.append(_kwargs)
        return "f" * 40

    def fake_reserve_recovery_identity(*_args):
        reservation_sequence.append("covered")
        recovery_identity_reservations.append(_args[-2:])
        return True

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
    preparation_script.reserve_recovery_identity = fake_reserve_recovery_identity
    preparation_script.reserve_preparation_identity = lambda *_args: True
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
        assert calls == {"source_ci": 2, "reserve": 1, "create": 1}

        cleanup_calls = []
        preparation_script.delete_reservation_ref = lambda *_args: cleanup_calls.append(_args)
        calls = {"source_ci": 0, "reserve": 0, "create": 0}

        def fail_on_final_source_check(*_args, **_kwargs):
            calls["source_ci"] += 1
            if calls["source_ci"] == 2:
                raise preparation_script.PreparationError("source metadata drift")
            return "2026-01-01T00:00:00Z"

        preparation_script.source_ci_ready = fail_on_final_source_check
        with tempfile.TemporaryDirectory() as drift_directory:
            expect_error(preparation_script.create, Namespace(
                api_root="https://api.github.test",
                token="token",
                repository="IvanLi-CN/dockrev",
                pr_number=42,
                exact_version=None,
                output=Path(drift_directory) / "release-intent.json",
            ))
        assert len(cleanup_calls) == 1
        assert calls["create"] == 0

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

        preparation_script.pull_request = lambda *_args: {
            "state": "open",
            "base": {"ref": "main"},
            "head": {"sha": prep_sha, "ref": "recovery/version-only", "repo": {"full_name": "IvanLi-CN/dockrev"}},
            "labels": [{"name": "type:patch"}, {"name": "channel:rc"}],
        }
        preparation_script.source_ci_ready = fake_source_ci
        covered_merge_sha = "c" * 40
        moved_covered_head_sha = "d" * 40
        covered_identity_marker = False
        reservation_sequence.clear()

        def immutable_covered_version(*_args):
            assert _args[-1] == covered_merge_sha
            return "0.1.1-beta.1"

        preparation_script.current_version = immutable_covered_version
        def fake_version_only_api(_api_root, _token, _method, path, _payload=None):
            if path.endswith(f"/commits/{prep_sha}"):
                return {
                    "commit": {
                        "verification": {"verified": True},
                        "message": (
                            "VERSION-only recovery\n\n"
                            "Covered-Product-Merge-SHA: " + covered_merge_sha + "\n"
                            "Product-Version: 0.1.1-rc.1\n"
                            "Release-Intent: type:patch channel:rc\n"
                            "Release-Mode: version-only-release-pr"
                        )
                    }
                }
            if path.endswith(f"/commits/{covered_merge_sha}/pulls"):
                return [{
                    "number": 41,
                    "state": "closed",
                    "merged_at": "2026-01-01T00:00:00Z",
                    "merge_commit_sha": covered_merge_sha,
                    "base": {"ref": "main"},
                    "head": {"sha": moved_covered_head_sha},
                    "labels": [{"name": "type:none"}, {"name": "channel:stable"}],
                }]
            if path.endswith(f"/git/ref/heads/release-recovery%2F{covered_merge_sha}"):
                if covered_identity_marker == "foreign":
                    return {"object": {"sha": "9" * 40}}
                if covered_identity_marker == "owned":
                    return {"object": {"sha": prep_sha}}
                raise preparation_script.PreparationError("GitHub API failed: 404:")
            if path.endswith("/git/ref/heads/release-reservation%2Fv0.1.1-beta.1"):
                if covered_identity_marker == "foreign":
                    return {"object": {"sha": "a" * 40}}
                raise preparation_script.PreparationError("GitHub API failed: 404:")
            if path.endswith("/git/ref/heads/release-reservation%2Fv0.1.1-rc.1"):
                if covered_identity_marker == "owned":
                    return {"object": {"sha": "f" * 40}}
                raise preparation_script.PreparationError("GitHub API failed: 404:")
            if path.endswith("/commits/" + "a" * 40):
                return {
                    "parents": [{"sha": covered_merge_sha}],
                    "commit": {
                        "message": (
                            "Reserve release version v0.1.1-beta.1\n\n"
                            "Release-Reservation-Version: 0.1.1-beta.1\n"
                            "Release-Reservation-PR: 41\n"
                            f"Release-Reservation-Source-SHA: {covered_merge_sha}"
                        )
                    },
                }
            if path.endswith("/commits/" + "f" * 40):
                return {
                    "parents": [{"sha": covered_merge_sha}],
                    "commit": {
                        "message": (
                            "Reserve release version v0.1.1-rc.1\n\n"
                            "Release-Reservation-Version: 0.1.1-rc.1\n"
                            "Release-Reservation-PR: 42\n"
                            f"Release-Reservation-Source-SHA: {covered_merge_sha}\n"
                            f"Release-Reservation-Identity-SHA: {prep_sha}\n"
                            "Release-Reservation-Intent: type:patch channel:rc\n"
                            "Release-Reservation-Mode: version-only-release-pr"
                        )
                    },
                }
            if path.endswith("/git/ref/tags/v0.1.1-beta.1"):
                raise preparation_script.PreparationError("GitHub API failed: 404:")
            if path.endswith("/pulls/42/files?per_page=100&page=1"):
                return [{"filename": "VERSION"}]
            raise AssertionError(path)

        preparation_script.api_request = fake_version_only_api
        output = Path(directory) / "version-only.json"
        preparation_script.create(Namespace(
            api_root="https://api.github.test",
            token="token",
            repository="IvanLi-CN/dockrev",
            pr_number=42,
            exact_version=None,
            output=output,
        ))
        result = json.loads(output.read_text(encoding="utf-8"))
        assert result["release_mode"] == "version-only-release-pr"
        assert result["skipped"] == "already-version-only"
        assert calls["reserve"] == 1
        assert reserved_sources[-1] == covered_merge_sha
        assert reservation_kwargs[-1] == {
            "identity_sha": prep_sha,
            "release_intent": "type:patch channel:rc",
            "release_mode": "version-only-release-pr",
        }
        assert recovery_identity_reservations[-1] == (covered_merge_sha, prep_sha)
        assert reservation_sequence == ["covered", "version"]

        covered_identity_marker = False
        assert preparation_script.covered_product_has_existing_identity(
            "https://api.github.test",
            "token",
            "IvanLi-CN/dockrev",
            covered_merge_sha,
            "0.1.1-beta.1",
        ) is False

        preparation_script.reserve_recovery_identity = lambda *_args: False
        expect_error(preparation_script.create, Namespace(
            api_root="https://api.github.test",
            token="token",
            repository="IvanLi-CN/dockrev",
            pr_number=42,
            exact_version=None,
            output=Path(directory) / "orphaned-recovery.json",
        ))
        preparation_script.reserve_recovery_identity = fake_reserve_recovery_identity

        covered_identity_marker = "foreign"
        reservations_before = calls["reserve"]
        expect_error(preparation_script.create, Namespace(
            api_root="https://api.github.test",
            token="token",
            repository="IvanLi-CN/dockrev",
            pr_number=42,
            exact_version=None,
            output=Path(directory) / "existing-identity.json",
        ))
        assert calls["reserve"] == reservations_before
        covered_identity_marker = False

        original_version_only_source_ci = preparation_script.source_ci_ready
        try:
            def fail_recovery_ci(*_args, **_kwargs):
                calls["source_ci"] += 1
                if calls["source_ci"] % 2 == 0:
                    raise preparation_script.PreparationError("recovery CI is not ready")
                return "2026-01-01T00:00:00Z"

            preparation_script.source_ci_ready = fail_recovery_ci
            calls["source_ci"] = 0
            reservations_before = calls["reserve"]
            expect_error(preparation_script.create, Namespace(
                api_root="https://api.github.test",
                token="token",
                repository="IvanLi-CN/dockrev",
                pr_number=42,
                exact_version=None,
                output=Path(directory) / "failed-recovery-ci.json",
            ))
            assert calls["reserve"] == reservations_before + 1
            assert recovery_identity_reservations[-1] == (covered_merge_sha, prep_sha)
            assert cleanup_calls[-1][-2:] == ("0.1.1-rc.1", "f" * 40)
        finally:
            preparation_script.source_ci_ready = original_version_only_source_ci
finally:
    preparation_script.pull_request = original_pull_request
    preparation_script.api_request = original_preparation_api_request
    preparation_script.source_ci_ready = original_source_ci_ready
    preparation_script.current_version = original_current_version
    preparation_script.reserve_tag = original_reserve_tag
    preparation_script.delete_reservation_ref = original_delete_reservation_ref
    preparation_script.reserve_recovery_identity = original_reserve_recovery_identity
    preparation_script.reserve_preparation_identity = original_reserve_preparation_identity
    preparation_script.create_commit = original_create_commit
    preparation_script.inspect_commit = original_inspect_commit

original_preparation_pull_request = preparation_script.pull_request
original_preparation_api_request = preparation_script.api_request
try:
    preparation_script.pull_request = lambda *_args: {
        "state": "open",
        "base": {"ref": "main"},
        "head": {"sha": source_sha, "ref": "feature/release", "repo": {"full_name": "IvanLi-CN/dockrev"}},
        "labels": [{"name": "type:patch"}, {"name": "channel:stable"}],
    }
    preparation_script.api_request = lambda *_args, **_kwargs: {
        "commit": {
            "message": (
                "Prepare release identity\n\n"
                f"Source-SHA: {source_sha}\n"
                "Product-Version: 0.1.1\n"
                "Release-Intent: type:patch channel:stable\n"
                "Release-Mode: normal-preparation\n"
                f"Covered-Product-Merge-SHA: {source_sha}"
            )
        }
    }
    expect_error(
        preparation_script.create,
        Namespace(
            api_root="https://api.github.test",
            token="token",
            repository="IvanLi-CN/dockrev",
            pr_number=42,
            exact_version=None,
            output=Path(tempfile.mkdtemp()) / "mixed.json",
        ),
    )
finally:
    preparation_script.pull_request = original_preparation_pull_request
    preparation_script.api_request = original_preparation_api_request

captured = {}
original_graphql = preparation_script.graphql
def fake_graphql(_api_root, _token, _query, variables):
    captured["variables"] = variables
    return {"createCommitOnBranch": {"commit": {"oid": "d" * 40}}}

preparation_script.graphql = fake_graphql
assert preparation_script.create_commit(
    "https://api.github.test", "token", "IvanLi-CN/dockrev", "feature/release", source_sha, "0.1.1", labels,
    "2026-01-01T00:00:00Z",
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
            "commit": {"verification": {"verified": True}, "message": "Release-Mode: normal-preparation\nSource-SHA: " + source_sha + "\nSource-PR-Updated-At: 2026-01-01T00:00:00Z\nProduct-Version: 0.1.1\nRelease-Intent: type:patch channel:stable"},
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
        "commit": {"verification": {"verified": True}, "message": "Release-Mode: normal-preparation\nSource-SHA: " + source_sha + "\nSource-PR-Updated-At: 2026-01-01T00:00:00Z\nProduct-Version: 0.1.1\nRelease-Intent: type:patch channel:stable"},
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
        "Source-PR-Updated-At: 2026-01-01T00:00:00Z\n"
        "Product-Version: 0.1.1\n"
        "Release-Intent: type:patch channel:stable\n"
        "Release-Mode: normal-preparation"
    )

    def fake_completion_api(_api_root, _token, path):
        if path.endswith("/pulls/42"):
            return {
                "state": "open",
                "base": {"ref": "main", "sha": "e" * 40},
                "head": {"sha": prep_sha},
                "updated_at": "2026-01-03T00:00:00Z",
                "labels": [{"name": label} for label in labels_for_loader],
            }
        if path.endswith(f"/commits/{prep_sha}"):
            return {
                "parents": [{"sha": source_sha}],
                "files": [{"filename": "VERSION"}] if labels_for_loader[0] != "type:none" else [],
                "commit": {"verification": {"verified": True}, "message": preparation_message if labels_for_loader[0] != "type:none" else "Product change"},
            }
        if path.endswith("/pulls/42/files?per_page=100&page=1"):
            return [{"filename": "VERSION"}] if labels_for_loader[0] == "type:none" else []
        if path.endswith("/commits/" + source_sha):
            return {"parents": [], "files": [], "commit": {"message": "Product change"}}
        if "/contents/VERSION?ref=" in path:
            if path.endswith("ref=" + "e" * 40):
                raise completion.CompletionError("GitHub API failed: 404")
            value = "0.1.0" if labels_for_loader[0] == "type:none" or source_sha in path else "0.1.1"
            encoded = __import__("base64").b64encode(value.encode()).decode()
            return {"encoding": "base64", "content": encoded}
        if "/actions/workflows/ci-pr.yml/runs?per_page=100&page=" in path:
            return {"workflow_runs": [{"head_sha": source_sha, "status": "completed", "conclusion": "success", "pull_requests": [{"number": 42}]}]}
        if "/actions/workflows/label-gate.yml/runs?per_page=100&page=" in path:
            return {"workflow_runs": [{"head_sha": source_sha, "status": "completed", "conclusion": "success", "created_at": "2026-01-02T00:00:00Z", "pull_requests": [{"number": 42, "head": {"sha": source_sha}}]}]}
        if path.endswith("/git/ref/tags/v0.1.1"):
            if tag_exists:
                return {"object": {"sha": prep_sha, "type": "commit"}}
            raise completion.CompletionError("GitHub API failed: 404")
        if path.endswith("/git/ref/heads/release-reservation%2Fv0.1.1"):
            return {"object": {"sha": "f" * 40, "type": "commit"}}
        if path.endswith("/commits/" + "f" * 40):
            return {
                "parents": [{"sha": source_sha}],
                "commit": {"message": "Reserve release version v0.1.1\n\nRelease-Reservation-Version: 0.1.1\nRelease-Reservation-PR: 42\nRelease-Reservation-Source-SHA: " + source_sha},
            }
        if path.endswith(f"/git/ref/heads/release-preparation%2F42%2F{source_sha}"):
            return {"object": {"sha": prep_sha}}
        if "/pulls?state=" in path:
            return []
        raise AssertionError(f"unexpected completion API path: {path}")

    completion.api_json = fake_completion_api
    loaded = completion.load_github_completion("https://api.github.test", "token", "IvanLi-CN/dockrev", 42)
    assert completion.validate_completion(loaded)["status"] == "pass"
    normal_preparation_message = preparation_message
    preparation_message = normal_preparation_message + "\nCovered-Product-Merge-SHA: " + source_sha
    expect_error(completion.load_github_completion, "https://api.github.test", "token", "IvanLi-CN/dockrev", 42)
    preparation_message = normal_preparation_message
    expect_error(completion.load_github_completion, "https://api.github.test", "token", "IvanLi-CN/dockrev", 42, "c" * 40)
    tag_exists = True
    expect_error(completion.validate_completion, completion.load_github_completion("https://api.github.test", "token", "IvanLi-CN/dockrev", 42))
    labels_for_loader = ["type:none", "channel:stable"]
    assert completion.load_github_completion("https://api.github.test", "token", "IvanLi-CN/dockrev", 42) == {
        "labels": labels_for_loader,
        "changed_files": ["VERSION"],
        "version_bootstrap": True,
        "base_version_exists": False,
        "version_file": "0.1.0",
    }
finally:
    completion.api_json = original_completion_api_json

original_completion_api_json = completion.api_json
try:
    completion_moved_covered_head_sha = "d" * 40
    version_only_completion_message = (
        "VERSION-only recovery\n\n"
        f"Covered-Product-Merge-SHA: {covered_merge_sha}\n"
        "Product-Version: 0.1.1-rc.1\n"
        "Release-Intent: type:patch channel:rc\n"
        "Release-Mode: version-only-release-pr"
    )
    recovery_identity_matches = True

    def fake_version_only_completion_api(_api_root, _token, path):
        if path.endswith("/pulls/42"):
            return {
                "state": "open",
                "base": {"ref": "main"},
                "head": {"sha": prep_sha},
                "updated_at": "2026-01-01T00:00:00Z",
                "labels": [{"name": "type:patch"}, {"name": "channel:rc"}],
            }
        if path.endswith(f"/commits/{prep_sha}"):
            return {
                "parents": [{"sha": covered_merge_sha}],
                "files": [{"filename": "VERSION"}],
                "commit": {
                    "verification": {"verified": True},
                    "message": version_only_completion_message,
                },
            }
        if path.endswith(f"/commits/{covered_merge_sha}/pulls"):
            return [{
                "number": 41,
                "state": "closed",
                "merged_at": "2026-01-01T00:00:00Z",
                "merge_commit_sha": covered_merge_sha,
                "base": {"ref": "main"},
                "head": {"sha": completion_moved_covered_head_sha},
                "updated_at": "2026-01-01T00:00:00Z",
                "labels": [{"name": "type:none"}, {"name": "channel:stable"}],
            }]
        if path.endswith(f"/pulls/42/files?per_page=100&page=1"):
            return [{"filename": "VERSION"}]
        if path.endswith(f"/pulls/41/files?per_page=100&page=1"):
            return []
        if "/contents/VERSION?ref=" in path:
            version = b"0.1.1-beta.1" if covered_merge_sha in path else b"0.1.1-rc.1"
            encoded = __import__("base64").b64encode(version).decode()
            return {"encoding": "base64", "content": encoded}
        if path.endswith(f"/git/ref/heads/release-recovery%2F{covered_merge_sha}"):
            if recovery_identity_matches is None:
                raise completion.CompletionError("GitHub API failed: 404")
            return {"object": {"sha": prep_sha if recovery_identity_matches else "9" * 40}}
        if path.endswith("/git/ref/heads/release-reservation%2Fv0.1.1-beta.1"):
            raise completion.CompletionError("GitHub API failed: 404")
        if path.endswith("/git/ref/tags/v0.1.1-beta.1"):
            raise completion.CompletionError("GitHub API failed: 404")
        if "/actions/workflows/ci-pr.yml/runs?per_page=100&page=" in path:
            return {"workflow_runs": [{"head_sha": prep_sha, "status": "completed", "conclusion": "success", "pull_requests": [{"number": 42}]}]}
        if "/actions/workflows/label-gate.yml/runs?per_page=100&page=" in path:
            return {"workflow_runs": [{"head_sha": "e" * 40, "status": "completed", "conclusion": "success", "created_at": "2026-01-02T00:00:00Z", "pull_requests": [{"number": 42, "head": {"sha": prep_sha}}]}]}
        if path.endswith("/git/ref/tags/v0.1.1-rc.1"):
            raise completion.CompletionError("GitHub API failed: 404")
        if path.endswith("/git/ref/heads/release-reservation%2Fv0.1.1-rc.1"):
            return {"object": {"sha": "f" * 40}}
        if path.endswith("/commits/" + "f" * 40):
            return {
                "parents": [{"sha": covered_merge_sha}],
                "commit": {"message": "Reserve release version v0.1.1-rc.1\n\nRelease-Reservation-Version: 0.1.1-rc.1\nRelease-Reservation-PR: 42\nRelease-Reservation-Source-SHA: " + covered_merge_sha + "\nRelease-Reservation-Identity-SHA: " + prep_sha + "\nRelease-Reservation-Intent: type:patch channel:rc\nRelease-Reservation-Mode: version-only-release-pr"},
            }
        if "/pulls?state=" in path:
            return []
        raise AssertionError(f"unexpected version-only API path: {path}")

    completion.api_json = fake_version_only_completion_api
    version_only_loaded = completion.load_github_completion("https://api.github.test", "token", "IvanLi-CN/dockrev", 42)
    assert completion.validate_completion(version_only_loaded)["mode"] == "version-only-release-pr"
    recovery_identity_matches = False
    expect_error(completion.load_github_completion, "https://api.github.test", "token", "IvanLi-CN/dockrev", 42)
    recovery_identity_matches = None
    expect_error(
        completion.validate_completion,
        completion.load_github_completion("https://api.github.test", "token", "IvanLi-CN/dockrev", 42),
    )
    recovery_identity_matches = True
    version_only_completion_message += f"\nSource-SHA: {completion_moved_covered_head_sha}"
    expect_error(completion.load_github_completion, "https://api.github.test", "token", "IvanLi-CN/dockrev", 42)
finally:
    completion.api_json = original_completion_api_json

original_completion_api_json = completion.api_json
try:
    page_calls = []

    def fake_paged_runs(_api_root, _token, path):
        page_calls.append(path)
        if path.endswith("page=1"):
            return {"workflow_runs": [{"pull_requests": []}] * 100}
        return {"workflow_runs": [{"head_sha": source_sha, "pull_requests": [{"number": 42}]}]}

    completion.api_json = fake_paged_runs
    assert len(completion.workflow_runs_for_pr("https://api.github.test", "token", "IvanLi-CN/dockrev", "ci-pr.yml", 42, source_sha)) == 1
    assert page_calls[-1].endswith("page=2")
finally:
    completion.api_json = original_completion_api_json

reservation_calls = []
original_preparation_api_request = preparation_script.api_request
try:
    def fake_reservation_api(_api_root, _token, method, path, payload=None):
        reservation_calls.append((method, path, payload))
        if method == "GET" and path.endswith("/git/ref/heads/release-reservation%2Fv0.1.1"):
            raise preparation_script.PreparationError("GitHub API GET ref failed: 404: missing")
        if method == "GET" and path.endswith(f"/commits/{source_sha}"):
            return {"commit": {"tree": {"sha": "e" * 40}}}
        if method == "POST" and path.endswith("/git/commits"):
            assert payload["tree"] == "e" * 40
            assert payload["parents"] == [source_sha]
            assert payload["message"] == (
                "Reserve release version v0.1.1\n\n"
                "Release-Reservation-Version: 0.1.1\n"
                "Release-Reservation-PR: 42\n"
                f"Release-Reservation-Source-SHA: {source_sha}"
            )
            return {"sha": "f" * 40}
        if method == "POST" and path.endswith("/git/refs"):
            return {"ref": "refs/heads/release-reservation/v0.1.1", "object": {"sha": "f" * 40}}
        raise AssertionError((method, path, payload))

    preparation_script.api_request = fake_reservation_api
    preparation_script.reserve_version_ref(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "0.1.1", 42, source_sha
    )
    assert reservation_calls[-1] == (
        "POST",
        "/repos/IvanLi-CN/dockrev/git/refs",
        {"ref": "refs/heads/release-reservation/v0.1.1", "sha": "f" * 40},
    )
finally:
    preparation_script.api_request = original_preparation_api_request

reservation_delete_calls = []
original_preparation_api_request = preparation_script.api_request
try:
    def fake_delete_reservation_api(_api_root, _token, method, path, _payload=None):
        if method == "GET":
            return {"object": {"sha": "f" * 40}}
        if method == "DELETE":
            reservation_delete_calls.append((method, path))
            return {}
        raise AssertionError((method, path))

    preparation_script.api_request = fake_delete_reservation_api
    preparation_script.delete_reservation_ref(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "0.1.1", "f" * 40
    )
    assert reservation_delete_calls == [
        ("DELETE", "/repos/IvanLi-CN/dockrev/git/refs/heads/release-reservation%2Fv0.1.1")
    ]
finally:
    preparation_script.api_request = original_preparation_api_request

recovery_ref_calls = []
original_preparation_api_request = preparation_script.api_request
try:
    recovery_ref_state = {"sha": None}
    recovery_create_collision = None

    def fake_recovery_ref_api(_api_root, _token, method, path, payload=None):
        recovery_ref_calls.append((method, path, payload))
        if path.endswith("/git/ref/heads/release-recovery%2F" + "c" * 40):
            if recovery_ref_state["sha"] is None:
                raise preparation_script.PreparationError("GitHub API GET ref failed: 404: missing")
            return {"object": {"sha": recovery_ref_state["sha"]}}
        if method == "POST" and path.endswith("/git/refs"):
            assert payload == {"ref": "refs/heads/release-recovery/" + "c" * 40, "sha": prep_sha}
            if recovery_create_collision is not None:
                recovery_ref_state["sha"] = recovery_create_collision
                raise preparation_script.PreparationError("GitHub API POST ref failed: 422: already exists")
            recovery_ref_state["sha"] = prep_sha
            return {"ref": payload["ref"], "object": {"sha": prep_sha}}
        if method == "DELETE" and path.endswith("/git/refs/heads/release-recovery%2F" + "c" * 40):
            recovery_ref_state["sha"] = None
            return {}
        raise AssertionError((method, path, payload))

    preparation_script.api_request = fake_recovery_ref_api
    assert preparation_script.reserve_recovery_identity(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "c" * 40, prep_sha
    ) is True
    assert preparation_script.reserve_recovery_identity(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "c" * 40, prep_sha
    ) is False
    recovery_ref_state["sha"] = None
    recovery_create_collision = prep_sha
    assert preparation_script.reserve_recovery_identity(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", "c" * 40, prep_sha
    ) is False
    recovery_ref_state["sha"] = None
    recovery_create_collision = "d" * 40
    expect_error(
        preparation_script.reserve_recovery_identity,
        "https://api.github.test",
        "token",
        "IvanLi-CN/dockrev",
        "c" * 40,
        prep_sha,
    )
    assert recovery_ref_state["sha"] == "d" * 40
finally:
    preparation_script.api_request = original_preparation_api_request

preparation_ref_calls = []
original_preparation_api_request = preparation_script.api_request
try:
    preparation_ref_state = {"sha": None}
    preparation_create_collision = None

    def fake_preparation_ref_api(_api_root, _token, method, path, payload=None):
        preparation_ref_calls.append((method, path, payload))
        expected_ref = "/git/ref/heads/release-preparation%2F42%2F" + source_sha
        if path.endswith(expected_ref):
            if preparation_ref_state["sha"] is None:
                raise preparation_script.PreparationError("GitHub API GET ref failed: 404: missing")
            return {"object": {"sha": preparation_ref_state["sha"]}}
        if method == "POST" and path.endswith("/git/refs"):
            assert payload == {
                "ref": "refs/heads/release-preparation/42/" + source_sha,
                "sha": prep_sha,
            }
            if preparation_create_collision is not None:
                preparation_ref_state["sha"] = preparation_create_collision
                raise preparation_script.PreparationError("GitHub API POST ref failed: 422: already exists")
            preparation_ref_state["sha"] = prep_sha
            return {"ref": payload["ref"], "object": {"sha": prep_sha}}
        raise AssertionError((method, path, payload))

    preparation_script.api_request = fake_preparation_ref_api
    assert preparation_script.reserve_preparation_identity(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", 42, source_sha, prep_sha
    ) is True
    assert preparation_script.reserve_preparation_identity(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", 42, source_sha, prep_sha
    ) is False
    preparation_ref_state["sha"] = None
    preparation_create_collision = prep_sha
    assert preparation_script.reserve_preparation_identity(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", 42, source_sha, prep_sha
    ) is False
    preparation_ref_state["sha"] = None
    preparation_create_collision = "d" * 40
    expect_error(
        preparation_script.reserve_preparation_identity,
        "https://api.github.test",
        "token",
        "IvanLi-CN/dockrev",
        42,
        source_sha,
        prep_sha,
    )
finally:
    preparation_script.api_request = original_preparation_api_request

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
        "covered_product_version": "0.1.0",
        "covered_product_merged": True,
    "verified": True,
    },
    "source_checks": source_checks,
    "tag_reserved": True,
}
assert completion.validate_completion(version_only)["status"] == "pass"
expect_error(completion.validate_completion, {**version_only, "changed_files": []})
expect_error(
    policy.validate_version_only,
    version_only["changed_files"],
    {**version_only["provenance"], "product_version": "0.1.1-beta.1"},
)
expect_error(
    policy.validate_version_only,
    version_only["changed_files"],
    {**version_only["provenance"], "product_version": "9.9.9"},
)
rc_version_only = {
    **version_only,
    "labels": ["type:patch", "channel:rc"],
    "version_file": "0.1.1-rc.1",
    "provenance": {
        **version_only["provenance"],
        "covered_product_version": "0.1.1-beta.1",
        "product_version": "0.1.1-rc.1",
        "release_intent": "type:patch channel:rc",
    },
}
assert completion.validate_completion(rc_version_only)["status"] == "pass"
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
rc_identity_payload = {
    **identity_payload,
    "version": "0.1.1-rc.1",
    "version_file": "0.1.1-rc.1",
    "intent": rc_labels,
}
resolved_rc = identity.resolve_from_payload(rc_identity_payload)
assert resolved_rc["channel"] == "rc" and resolved_rc["release_tag"] == "v0.1.1-rc.1"
expect_error(identity.resolve_from_payload, {**identity_payload, "version": "0.1.1-beta.1"})
expect_error(identity.resolve_from_payload, {"labels": ["type:none", "channel:stable"], "release_mode": "normal-preparation"})
expect_error(identity.resolve_from_payload, {**identity_payload, "covered_product_merge_sha": source_sha})

original_identity_api_json = identity.api_json
try:
    normal_merge_sha = "0" * 40
    normal_pr_head_sha = prep_sha
    normal_merge_parents = [{"sha": source_sha}]
    normal_reservation_sha = "8" * 40
    normal_tree_sha = "9" * 40

    identity.api_json = lambda *_args, **_kwargs: {
        "encoding": "base64",
        "content": "MC4xLjAK\n",
    }
    assert identity.version_at_commit(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", source_sha
    ) == "0.1.0"

    def fake_identity_api(_api_root, _token, path):
        if path.endswith(f"/commits/{normal_merge_sha}/pulls"):
            return [{
                "number": 42,
                "state": "closed",
                "merged_at": "2026-01-01T00:00:00Z",
                "merge_commit_sha": normal_merge_sha,
                "base": {"ref": "main"},
                "head": {"sha": normal_pr_head_sha},
                "labels": [{"name": "type:patch"}, {"name": "channel:stable"}],
            }]
        if path.endswith(f"/commits/{normal_merge_sha}"):
            return {
                "parents": normal_merge_parents,
                "files": [{"filename": "VERSION"}],
                "commit": {"message": "Merge release preparation", "tree": {"sha": normal_tree_sha}},
            }
        if path.endswith(f"/commits/{prep_sha}"):
            return {
                "parents": [{"sha": source_sha}],
                "files": [{"filename": "VERSION"}],
                "commit": {
                    "verification": {"verified": True},
                    "tree": {"sha": normal_tree_sha},
                    "message": (
                        "Prepare release identity\n\n"
                        f"Source-SHA: {source_sha}\n"
                        "Source-PR-Updated-At: 2026-01-01T00:00:00Z\n"
                        "Product-Version: 0.1.1\n"
                        "Release-Intent: type:patch channel:stable\n"
                        "Release-Mode: normal-preparation"
                    ),
                },
            }
        if path.endswith(f"/commits/{source_sha}"):
            return {"parents": [], "files": [{"filename": "src/lib.rs"}], "commit": {"message": "Product change"}}
        if "/contents/VERSION?ref=" in path:
            version = "0.1.0" if source_sha in path else "0.1.1"
            encoded = __import__("base64").b64encode(version.encode()).decode() + "\n"
            return {"encoding": "base64", "content": encoded}
        if path.endswith("/git/ref/heads/release-reservation%2Fv0.1.1"):
            return {"object": {"sha": normal_reservation_sha}}
        if path.endswith(f"/commits/{normal_reservation_sha}"):
            return {
                "parents": [{"sha": source_sha}],
                "commit": {"message": (
                    "Reserve release version v0.1.1\n\n"
                    "Release-Reservation-Version: 0.1.1\n"
                    "Release-Reservation-PR: 42\n"
                    f"Release-Reservation-Source-SHA: {source_sha}"
                )},
            }
        if path.endswith(f"/git/ref/heads/release-preparation%2F42%2F{source_sha}"):
            return {"object": {"sha": prep_sha}}
        raise AssertionError(f"unexpected identity API path: {path}")

    identity.api_json = fake_identity_api
    resolved_api = identity.resolve_github("https://api.github.test", "token", "IvanLi-CN/dockrev", normal_merge_sha)
    assert resolved_api["release_tag"] == "v0.1.1"
    normal_pr_head_sha = "7" * 40
    assert identity.resolve_github(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", normal_merge_sha
    )["preparation_commit_sha"] == prep_sha
    def fake_mixed_identity_api(_api_root, _token, path):
        payload = fake_identity_api(_api_root, _token, path)
        if path.endswith(f"/commits/{prep_sha}"):
            payload["commit"]["message"] += f"\nCovered-Product-Merge-SHA: {source_sha}"
        return payload

    identity.api_json = fake_mixed_identity_api
    expect_error(identity.resolve_github, "https://api.github.test", "token", "IvanLi-CN/dockrev", normal_merge_sha)
finally:
    identity.api_json = original_identity_api_json

bootstrap_merge_sha = "2" * 40
bootstrap_parent_sha = "3" * 40
bootstrap_source_sha = "4" * 40
original_identity_api_json = identity.api_json
try:
    def fake_bootstrap_identity_api(_api_root, _token, path):
        if path.endswith(f"/commits/{bootstrap_merge_sha}/pulls"):
            return [{
                "number": 44,
                "state": "closed",
                "merged_at": "2026-01-01T00:00:00Z",
                "merge_commit_sha": bootstrap_merge_sha,
                "base": {"ref": "main", "sha": bootstrap_parent_sha},
                "head": {"sha": bootstrap_source_sha},
                "labels": [{"name": "type:none"}, {"name": "channel:stable"}],
            }]
        if path.endswith(f"/commits/{bootstrap_source_sha}"):
            return {"parents": [], "files": [], "commit": {"message": "Bootstrap VERSION"}}
        if path.endswith(f"/pulls/44/files?per_page=100&page=1"):
            return [{"filename": "VERSION"}, {"filename": "docs/release.md"}]
        if path.endswith(f"/contents/VERSION?ref={bootstrap_parent_sha}"):
            raise identity.IdentityError("GitHub API failed: 404: missing")
        if path.endswith(f"/contents/VERSION?ref={bootstrap_merge_sha}"):
            return {"encoding": "base64", "content": "MC4xLjAK\n"}
        if path.endswith("/git/ref/heads/release-reservation%2Fv0.1.0"):
            raise identity.IdentityError("GitHub API failed: 404")
        raise AssertionError(f"unexpected bootstrap identity API path: {path}")

    identity.api_json = fake_bootstrap_identity_api
    bootstrap_identity = identity.resolve_github(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", bootstrap_merge_sha
    )
    assert bootstrap_identity == {
        "release_enabled": False,
        "merge_commit_sha": bootstrap_merge_sha,
        "pull_request": 44,
        "reason": "version-bootstrap",
    }
finally:
    identity.api_json = original_identity_api_json

no_identity_merge_sha = "5" * 40
no_identity_head_sha = "6" * 40
original_identity_api_json = identity.api_json
try:
    def fake_no_identity_api(_api_root, _token, path):
        if path.endswith(f"/commits/{no_identity_merge_sha}/pulls"):
            return [{
                "number": 45,
                "state": "closed",
                "merged_at": "2026-01-02T00:00:00Z",
                "merge_commit_sha": no_identity_merge_sha,
                "base": {"ref": "main", "sha": "7" * 40},
                "head": {"sha": no_identity_head_sha},
                "labels": [{"name": "type:minor"}, {"name": "channel:beta"}],
            }]
        if path.endswith(f"/commits/{no_identity_merge_sha}"):
            return {
                "parents": [{"sha": "7" * 40}, {"sha": no_identity_head_sha}],
                "files": [{"filename": "src/lib.rs"}],
                "commit": {"message": "Merge product change"},
            }
        if path.endswith(f"/commits/{'7' * 40}"):
            return {"parents": [], "files": [{"filename": "src/lib.rs"}], "commit": {"message": "Main parent"}}
        if path.endswith(f"/commits/{no_identity_head_sha}"):
            return {"parents": [], "files": [], "commit": {"message": "Product change"}}
        if path.endswith(f"/contents/VERSION?ref={no_identity_merge_sha}"):
            return {"encoding": "base64", "content": "MC4xLjAK\n"}
        if path.endswith("/git/ref/heads/release-reservation%2Fv0.1.0"):
            raise identity.IdentityError("GitHub API failed: 404")
        raise AssertionError(f"unexpected no-identity API path: {path}")

    identity.api_json = fake_no_identity_api
    no_identity = identity.resolve_github(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", no_identity_merge_sha
    )
    assert no_identity["reason"] == "no-release-identity"
    assert no_identity["type"] == "minor"
    assert no_identity["channel"] == "beta"
    assert no_identity["intent"]["type_label"] == "type:minor"
finally:
    identity.api_json = original_identity_api_json

version_only_merge_sha = "d" * 40
version_only_release_head_sha = "e" * 40
version_only_covered_merge_sha = "f" * 40
version_only_covered_head_sha = "1" * 40
version_only_moved_head_sha = "2" * 40
version_only_reservation_sha = "3" * 40
version_only_merge_tree_sha = "6" * 40
version_only_identity_tree_sha = version_only_merge_tree_sha
original_identity_api_json = identity.api_json
try:
    recovery_identity_matches = True
    recovery_merge_files = [{"filename": "VERSION"}]
    recovery_merge_parents = [{"sha": "0" * 40}]
    covered_identity_state = {"kind": None, "after_initial_check": None, "reads": 0}
    reservation_ref_state = {"reads": 0, "rebound": False}

    def fake_version_only_identity_api(_api_root, _token, path):
        if path.endswith(f"/commits/{version_only_merge_sha}/pulls"):
            return [{
                "number": 43,
                "state": "closed",
                "merged_at": "2026-01-01T00:00:00Z",
                "merge_commit_sha": version_only_merge_sha,
                "base": {"ref": "main"},
                "head": {"sha": version_only_moved_head_sha},
                "labels": [{"name": "type:patch"}, {"name": "channel:rc"}],
            }]
        if path.endswith(f"/commits/{version_only_merge_sha}"):
            return {
                "parents": recovery_merge_parents,
                "files": recovery_merge_files,
                "commit": {"message": "Merge version-only recovery", "tree": {"sha": version_only_merge_tree_sha}},
            }
        if path.endswith("/git/ref/heads/release-reservation%2Fv0.1.1-rc.1"):
            reservation_ref_state["reads"] += 1
            if reservation_ref_state["rebound"] and reservation_ref_state["reads"] > 1:
                return {"object": {"sha": "4" * 40}}
            return {"object": {"sha": version_only_reservation_sha}}
        if path.endswith(f"/commits/{version_only_reservation_sha}"):
            return {
                "parents": [{"sha": version_only_covered_merge_sha}],
                "commit": {
                    "message": (
                        "Reserve release version v0.1.1-rc.1\n\n"
                        "Release-Reservation-Version: 0.1.1-rc.1\n"
                        "Release-Reservation-PR: 43\n"
                        f"Release-Reservation-Source-SHA: {version_only_covered_merge_sha}\n"
                        f"Release-Reservation-Identity-SHA: {version_only_release_head_sha}\n"
                        "Release-Reservation-Intent: type:patch channel:rc\n"
                        "Release-Reservation-Mode: version-only-release-pr"
                    )
                },
            }
        if path.endswith(f"/git/ref/heads/release-recovery%2F{version_only_covered_merge_sha}"):
            if recovery_identity_matches is None:
                raise identity.IdentityError("GitHub API failed: 404")
            return {"object": {"sha": version_only_release_head_sha if recovery_identity_matches else "4" * 40}}
        if path.endswith("/git/ref/heads/release-reservation%2Fv0.1.1-beta.1"):
            covered_identity_state["reads"] += 1
            covered_identity = covered_identity_state["kind"]
            if covered_identity is None and covered_identity_state["reads"] > 2:
                covered_identity = covered_identity_state["after_initial_check"]
            if covered_identity == "reservation":
                return {"object": {"sha": "5" * 40}}
            raise identity.IdentityError("GitHub API failed: 404")
        if path.endswith("/git/ref/tags/v0.1.1-beta.1"):
            covered_identity_state["reads"] += 1
            covered_identity = covered_identity_state["kind"]
            if covered_identity is None and covered_identity_state["reads"] > 2:
                covered_identity = covered_identity_state["after_initial_check"]
            if covered_identity == "tag":
                return {"object": {"sha": "5" * 40}}
            raise identity.IdentityError("GitHub API failed: 404")
        if path.endswith(f"/commits/{version_only_release_head_sha}"):
            return {
                "parents": [{"sha": version_only_covered_head_sha}],
                "files": [{"filename": "VERSION"}],
                "commit": {
                    "verification": {"verified": True},
                    "tree": {"sha": version_only_identity_tree_sha},
                    "message": (
                        "VERSION-only release\n\n"
                        f"Covered-Product-Merge-SHA: {version_only_covered_merge_sha}\n"
                        "Product-Version: 0.1.1-rc.1\n"
                        "Release-Intent: type:patch channel:rc\n"
                        "Release-Mode: version-only-release-pr"
                    ),
                },
            }
        if path.endswith(f"/commits/{version_only_covered_merge_sha}/pulls"):
            return [{
                "number": 41,
                "state": "closed",
                "merged_at": "2026-01-01T00:00:00Z",
                "merge_commit_sha": version_only_covered_merge_sha,
                "base": {"ref": "main"},
                "head": {"sha": version_only_covered_head_sha},
                "labels": [{"name": "type:none"}, {"name": "channel:stable"}],
            }]
        if path.endswith("/pulls/43/files?per_page=100&page=1"):
            return [{"filename": "VERSION"}]
        if "/contents/VERSION?ref=" in path:
            version = b"0.1.1-beta.1" if version_only_covered_merge_sha in path else b"0.1.1-rc.1"
            encoded = __import__("base64").b64encode(version).decode()
            return {"encoding": "base64", "content": encoded}
        raise AssertionError(f"unexpected version-only identity API path: {path}")

    identity.api_json = fake_version_only_identity_api
    resolved_version_only = identity.resolve_github(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", version_only_merge_sha
    )
    assert resolved_version_only["release_mode"] == "version-only-release-pr"
    assert resolved_version_only["source_sha"] == version_only_covered_merge_sha
    assert resolved_version_only["channel"] == "rc" and resolved_version_only["release_tag"] == "v0.1.1-rc.1"
    for covered_identity in ("reservation", "tag"):
        covered_identity_state = {"kind": covered_identity, "after_initial_check": None, "reads": 0}
        expect_error(
            identity.resolve_github,
            "https://api.github.test", "token", "IvanLi-CN/dockrev", version_only_merge_sha
        )
    covered_identity_state = {"kind": None, "after_initial_check": "reservation", "reads": 0}
    expect_error(
        identity.resolve_github,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", version_only_merge_sha
    )
    covered_identity_state = {"kind": None, "after_initial_check": None, "reads": 0}
    version_only_merge_tree_sha = "7" * 40
    expect_error(
        identity.resolve_github,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", version_only_merge_sha
    )
    version_only_merge_tree_sha = version_only_identity_tree_sha
    reservation_ref_state = {"reads": 0, "rebound": True}
    expect_error(
        identity.resolve_github,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", version_only_merge_sha
    )
    reservation_ref_state = {"reads": 0, "rebound": False}
    recovery_merge_files = [{"filename": "VERSION"}, {"filename": "docs/release.md"}]
    expect_error(
        identity.resolve_github,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", version_only_merge_sha
    )
    recovery_merge_files = [{"filename": "VERSION"}]
    recovery_identity_matches = False
    expect_error(
        identity.resolve_github,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", version_only_merge_sha
    )
    recovery_identity_matches = None
    expect_error(
        identity.resolve_github,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", version_only_merge_sha
    )
    recovery_identity_matches = True
    def fake_version_only_source_sha_api(_api_root, _token, path):
        payload = fake_version_only_identity_api(_api_root, _token, path)
        if path.endswith(f"/commits/{version_only_release_head_sha}"):
            payload["commit"]["message"] += f"\nSource-SHA: {version_only_covered_head_sha}"
        return payload

    identity.api_json = fake_version_only_source_sha_api
    expect_error(
        identity.resolve_github,
        "https://api.github.test", "token", "IvanLi-CN/dockrev", version_only_merge_sha
    )
finally:
    identity.api_json = original_identity_api_json

original_identity_api_json = identity.api_json
try:
    file_page_calls = []

    def fake_paged_identity_api(_api_root, _token, path):
        file_page_calls.append(path)
        if path.endswith("page=1"):
            return [{"filename": f"src/file-{index}.rs"} for index in range(100)]
        return [{"filename": "VERSION"}]

    identity.api_json = fake_paged_identity_api
    paged_files = identity.pull_request_changed_files(
        "https://api.github.test", "token", "IvanLi-CN/dockrev", 43
    )
    assert "VERSION" in paged_files and len(paged_files) == 101
    assert file_page_calls[-1].endswith("page=2")
finally:
    identity.api_json = original_identity_api_json

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
rc_failure = {
    **failure,
    "channel": "rc",
    "version": "0.1.1-rc.1",
    "tag": "v0.1.1-rc.1",
}
assert policy.validate_failure_context(rc_failure) == rc_failure
expect_error(policy.validate_failure_context, {**failure, "tag": "v0.1.0"})
expect_error(policy.validate_failure_context, {**failure, "artifact_names": []})
expect_error(policy.validate_failure_context, {**failure, "run_url": ""})
expect_error(policy.validate_failure_context, failure, expected_repository="IvanLi-CN/dockrev", expected_run_id="2")
expect_error(policy.validate_failure_context, failure, expected_repository="IvanLi-CN/dockrev", expected_run_id="1", expected_server="github.com", expected_attempt="2")
expect_error(policy.validate_failure_context, {**failure, "identity_failure_kind": "unknown"})
expect_error(policy.validate_failure_context, {**failure, "identity_failure_kind": "no-identity"})
expect_error(policy.validate_failure_context, {**failure, "source_sha": failure["merge_commit_sha"]})
step_failure = {
    **failure,
    "source_sha": failure["merge_commit_sha"],
    "identity_resolution_failed": False,
    "identity_failure_kind": "identity-step-failure",
    "recovery_instruction": "workflow_dispatch merge_sha=" + failure["merge_commit_sha"] + " recovery_reason=<required>",
}
assert policy.validate_failure_context(step_failure) == step_failure
rc_step_failure = {
    **rc_failure,
    "source_sha": rc_failure["merge_commit_sha"],
    "identity_resolution_failed": False,
    "identity_failure_kind": "identity-step-failure",
    "recovery_instruction": "workflow_dispatch merge_sha=" + rc_failure["merge_commit_sha"] + " recovery_reason=<required>",
}
assert policy.validate_failure_context(rc_step_failure) == rc_step_failure
expect_error(policy.validate_failure_context, {**step_failure, "identity_failure_kind": None})
resolver_failure = {
    **failure,
    "source_sha": prep_sha,
    "identity_resolution_failed": False,
    "identity_failure_kind": "resolver-error",
    "type": "unknown",
    "channel": "unknown",
    "recovery_instruction": "resolver-error: retry Release workflow after verifying merged identity",
}
assert policy.validate_failure_context(resolver_failure) == resolver_failure
expect_error(policy.validate_failure_context, {**resolver_failure, "type": "patch"})
identity_failure = {
    **failure,
    "source_sha": prep_sha,
    "version": "0.0.0",
    "tag": "v0.0.0",
    "identity_resolution_failed": True,
    "recovery_instruction": "create VERSION-only release PR Covered-Product-Merge-SHA=" + prep_sha,
}
assert policy.validate_failure_context(identity_failure) == identity_failure
expect_error(policy.validate_failure_context, {**identity_failure, "channel": "beta"})
expect_error(policy.validate_failure_context, {**identity_failure, "recovery_instruction": "workflow_dispatch merge_sha=" + prep_sha + " recovery_reason=<required>"})
failure_context = load("release_failure_context", ROOT / ".github/scripts/release_failure_context.py")
assert "recovery:" in failure_context.notification_summary(failure)
resolved_failure = failure_context.resolved_identity_failure_context(
    resolved,
    repository="IvanLi-CN/dockrev",
    server="https://github.com",
    run_id="1",
    attempt="2",
    event="push",
    ref="refs/heads/main",
    actor="tester",
)
assert resolved_failure["identity_failure_kind"] == "identity-step-failure"
assert policy.validate_failure_context(resolved_failure) == resolved_failure

with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    files = root / "files.json"
    provenance = root / "provenance.json"
    resolved_identity = root / "resolved-identity.json"
    failure_output = root / "failure-context.json"
    files.write_text(json.dumps(["VERSION"]))
    provenance.write_text(json.dumps(version_only["provenance"]))
    resolved_identity.write_text(json.dumps(resolved))
    subprocess.run(
        [
            sys.executable,
            str(ROOT / ".github/scripts/release_failure_context.py"),
            "--resolved-identity",
            str(resolved_identity),
            "--repository",
            "IvanLi-CN/dockrev",
            "--server",
            "https://github.com",
            "--run-id",
            "1",
            "--attempt",
            "2",
            "--event",
            "push",
            "--ref",
            "refs/heads/main",
            "--actor",
            "tester",
            "--output",
            str(failure_output),
        ],
        check=True,
    )
    cli_failure = json.loads(failure_output.read_text(encoding="utf-8"))
    assert cli_failure == resolved_failure
    assert policy.main.__name__ == "main"

print("PASS: PR label release fixtures")
