#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
python3 - <<'PY' "$repo_root/.github/scripts/release_snapshot.py" "$repo_root/.github/scripts/release_pr_comment.py"
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


def load_module(name: str, script_path: Path):
    spec = importlib.util.spec_from_file_location(name, script_path)
    module = importlib.util.module_from_spec(spec)
    assert spec is not None and spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


module = load_module("release_snapshot", Path(sys.argv[1]))
comment_module = load_module("release_pr_comment", Path(sys.argv[2]))


def run(*args: str, cwd: Path) -> str:
    result = subprocess.run(["git", *args], cwd=cwd, check=True, text=True, capture_output=True)
    return result.stdout.strip()


def make_pr(number: int, title: str, head_sha: str, labels: list[str]) -> dict[str, object]:
    return {
        "number": number,
        "title": title,
        "head": {"sha": head_sha},
        "labels": [{"name": label} for label in labels],
    }


def fake_push_git(original_git, notes_ref: str):
    def inner(*args: str, **kwargs: object):
        if args == ("push", "origin", notes_ref):
            return subprocess.CompletedProcess(["git", *args], 0, "", "")
        return original_git(*args, **kwargs)

    return inner


with tempfile.TemporaryDirectory(prefix="release-snapshot-versions-") as tmp:
    repo = Path(tmp)
    run("init", cwd=repo)
    run("config", "user.name", "Test User", cwd=repo)
    run("config", "user.email", "test@example.com", cwd=repo)
    run("checkout", "-b", "main", cwd=repo)
    (repo / "Cargo.toml").write_text('[package]\nname = "dockrev"\nversion = "0.1.0"\n')
    (repo / "README.md").write_text("base\n")
    run("add", "Cargo.toml", "README.md", cwd=repo)
    run("commit", "-m", "base", cwd=repo)
    run("tag", "0.1.0", cwd=repo)

    (repo / "README.md").write_text("one\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "one", cwd=repo)
    sha1 = run("rev-parse", "HEAD", cwd=repo)

    (repo / "README.md").write_text("two\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "two", cwd=repo)
    sha2 = run("rev-parse", "HEAD", cwd=repo)

    (repo / "README.md").write_text("three\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "three", cwd=repo)
    sha3 = run("rev-parse", "HEAD", cwd=repo)

    prs = {
        sha1: make_pr(101, "Patch release", sha1, ["type:patch", "channel:stable"]),
        sha2: make_pr(102, "Minor release", sha2, ["type:minor", "channel:stable"]),
        sha3: make_pr(103, "RC release", sha3, ["type:patch", "channel:rc"]),
    }

    original_cwd = Path.cwd()
    original_loader = module.load_pr_for_commit
    try:
        os.chdir(repo)
        module.load_pr_for_commit = lambda api_root, repository, token, target_sha, **kwargs: prs[target_sha]

        snapshot1 = module.build_snapshot(
            target_sha=sha1,
            repository="IvanLi-CN/dockrev",
            token="token",
            notes_ref=module.DEFAULT_NOTES_REF,
            registry="ghcr.io",
            api_root="https://api.github.com",
        )
        assert snapshot1["snapshot_source"] == "ci-main"
        assert snapshot1["next_stable_version"] == "0.1.1"
        assert snapshot1["release_tag"] == "0.1.1"
        assert snapshot1["tags_csv"] == "ghcr.io/ivanli-cn/dockrev:0.1.1,ghcr.io/ivanli-cn/dockrev:latest"
        run("notes", f"--ref={module.DEFAULT_NOTES_REF}", "add", "-f", "-m", json.dumps(snapshot1), sha1, cwd=repo)

        snapshot2 = module.build_snapshot(
            target_sha=sha2,
            repository="IvanLi-CN/dockrev",
            token="token",
            notes_ref=module.DEFAULT_NOTES_REF,
            registry="ghcr.io",
            api_root="https://api.github.com",
        )
        assert snapshot2["base_stable_version"] == "0.1.1"
        assert snapshot2["next_stable_version"] == "0.2.0"
        run("notes", f"--ref={module.DEFAULT_NOTES_REF}", "add", "-f", "-m", json.dumps(snapshot2), sha2, cwd=repo)

        snapshot3 = module.build_snapshot(
            target_sha=sha3,
            repository="IvanLi-CN/dockrev",
            token="token",
            notes_ref=module.DEFAULT_NOTES_REF,
            registry="ghcr.io",
            api_root="https://api.github.com",
        )
        assert snapshot3["base_stable_version"] == "0.2.0"
        assert snapshot3["next_stable_version"] == "0.2.1"
        assert snapshot3["app_effective_version"] == f"0.2.1-rc.{sha3[:7]}"
        assert snapshot3["release_tag"] == f"0.2.1-rc.{sha3[:7]}"
        assert snapshot3["release_prerelease"] is True
        run("notes", f"--ref={module.DEFAULT_NOTES_REF}", "add", "-f", "-m", json.dumps(snapshot3), sha3, cwd=repo)

        assert module.publication_tags(
            snapshot1,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            main_ref=sha3,
        ) == (
            "ghcr.io/ivanli-cn/dockrev:0.1.1,ghcr.io/ivanli-cn/dockrev:latest"
        )
        assert module.publication_tags(
            snapshot2,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            main_ref=sha3,
        ) == (
            "ghcr.io/ivanli-cn/dockrev:0.2.0,ghcr.io/ivanli-cn/dockrev:latest"
        )
        assert module.supervisor_publication_tags(
            snapshot2,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            main_ref=sha3,
        ) == (
            "ghcr.io/ivanli-cn/dockrev-supervisor:0.2.0,ghcr.io/ivanli-cn/dockrev-supervisor:latest"
        )
        assert module.publication_tags(
            snapshot3,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            main_ref=sha3,
        ) == f"ghcr.io/ivanli-cn/dockrev:{snapshot3['release_tag']}"

        docs_snapshot = module.build_snapshot(
            target_sha=sha1,
            repository="IvanLi-CN/dockrev",
            token="token",
            notes_ref=module.DEFAULT_NOTES_REF,
            registry="ghcr.io",
            api_root="https://api.github.com",
            pr=make_pr(104, "Docs only", sha1, ["type:docs", "channel:stable"]),
        )
        assert docs_snapshot["release_enabled"] is False
        assert docs_snapshot["release_tag"] == ""

        skip_snapshot = module.build_snapshot(
            target_sha=sha1,
            repository="IvanLi-CN/dockrev",
            token="token",
            notes_ref=module.DEFAULT_NOTES_REF,
            registry="ghcr.io",
            api_root="https://api.github.com",
            pr=make_pr(105, "Skip release", sha1, ["type:skip", "channel:stable"]),
        )
        assert skip_snapshot["release_enabled"] is False
        assert skip_snapshot["release_tag"] == ""

        try:
            module.build_snapshot(
                target_sha=sha1,
                repository="IvanLi-CN/dockrev",
                token="token",
                notes_ref=module.DEFAULT_NOTES_REF,
                registry="ghcr.io",
                api_root="https://api.github.com",
                pr=make_pr(106, "Broken labels", sha1, ["type:patch", "type:minor", "channel:stable"]),
            )
        except module.SnapshotError as exc:
            assert "Expected exactly 1 type:* label" in str(exc)
        else:
            raise AssertionError("expected invalid type labels to fail")

        run("tag", "0.1.1", sha1, cwd=repo)
        pending = module.pending_release_targets(
            module.DEFAULT_NOTES_REF,
            sha3,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
        )
        assert pending == [sha1, sha2, sha3], (pending, sha1, sha2, sha3)
        assert module.release_tag_points_to_target(snapshot1) is True
        assert module.release_tag_points_to_target(snapshot2) is False
        assert (
            module.release_state_for_target(
                snapshot1,
                publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
                override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
            )
            == "pending"
        )
    finally:
        module.load_pr_for_commit = original_loader
        os.chdir(original_cwd)


with tempfile.TemporaryDirectory(prefix="release-snapshot-target-only-") as tmp:
    repo = Path(tmp)
    run("init", cwd=repo)
    run("config", "user.name", "Test User", cwd=repo)
    run("config", "user.email", "test@example.com", cwd=repo)
    run("checkout", "-b", "main", cwd=repo)
    (repo / "Cargo.toml").write_text('[package]\nname = "dockrev"\nversion = "0.1.0"\n')
    (repo / "README.md").write_text("base\n")
    run("add", "Cargo.toml", "README.md", cwd=repo)
    run("commit", "-m", "base", cwd=repo)
    run("tag", "0.1.0", cwd=repo)

    (repo / "README.md").write_text("old merge\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "old merge", cwd=repo)
    old_sha = run("rev-parse", "HEAD", cwd=repo)

    (repo / "README.md").write_text("target merge\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "target merge", cwd=repo)
    target_sha = run("rev-parse", "HEAD", cwd=repo)

    original_cwd = Path.cwd()
    original_load_pr = module.load_pr_for_commit
    original_build_snapshot = module.build_snapshot
    original_git = module.git
    calls: list[str] = []

    def fake_build_snapshot(*, target_sha: str, **kwargs: object):
        calls.append(target_sha)
        if target_sha == old_sha:
            raise AssertionError("target-only mode should not materialize older snapshots")
        return {
            "schema_version": module.SNAPSHOT_SCHEMA_VERSION,
            "target_sha": target_sha,
            "pr_number": 202,
            "pr_title": "Target labeled merge",
            "registry": "ghcr.io",
            "pr_head_sha": "6" * 40,
            "type_label": "type:patch",
            "channel_label": "channel:stable",
            "release_bump": "patch",
            "release_channel": "stable",
            "release_enabled": True,
            "release_prerelease": False,
            "publish_latest": True,
            "image_name_lower": "ivanli-cn/dockrev",
            "supervisor_image_name_lower": "ivanli-cn/dockrev-supervisor",
            "base_stable_version": "0.1.0",
            "next_stable_version": "0.1.1",
            "app_effective_version": "0.1.1",
            "release_tag": "0.1.1",
            "tags_csv": "ghcr.io/ivanli-cn/dockrev:0.1.1,ghcr.io/ivanli-cn/dockrev:latest",
            "supervisor_tags_csv": "ghcr.io/ivanli-cn/dockrev-supervisor:0.1.1,ghcr.io/ivanli-cn/dockrev-supervisor:latest",
            "notes_ref": module.DEFAULT_NOTES_REF,
            "snapshot_source": "manual-backfill",
            "created_at": "2026-03-15T00:00:00Z",
        }

    os.chdir(repo)
    try:
        module.load_pr_for_commit = (
            lambda api_root, repository, token, commit_sha, **kwargs: {
                old_sha: make_pr(201, "Old merge", old_sha, ["type:patch", "channel:stable"]),
                target_sha: make_pr(202, "Target merge", target_sha, ["type:patch", "channel:stable"]),
            }.get(commit_sha)
        )
        module.build_snapshot = fake_build_snapshot
        module.git = fake_push_git(original_git, module.DEFAULT_NOTES_REF)
        exit_code = module.ensure_snapshot(
            argparse.Namespace(
                target_sha=target_sha,
                github_repository="IvanLi-CN/dockrev",
                github_token="token",
                notes_ref=module.DEFAULT_NOTES_REF,
                registry="ghcr.io",
                api_root="https://api.github.com",
                output=str(repo / "target-only.json"),
                max_attempts=1,
                target_only=True,
            )
        )
        assert exit_code == 0
        assert calls == [target_sha]
        assert module.read_snapshot(module.DEFAULT_NOTES_REF, old_sha) is None
        stored = module.read_snapshot(module.DEFAULT_NOTES_REF, target_sha)
        assert stored is not None
        assert stored["snapshot_source"] == "manual-backfill"
    finally:
        module.load_pr_for_commit = original_load_pr
        module.build_snapshot = original_build_snapshot
        module.git = original_git
        os.chdir(original_cwd)


with tempfile.TemporaryDirectory(prefix="release-snapshot-empty-notes-") as tmp:
    repo = Path(tmp)
    run("init", cwd=repo)
    run("config", "user.name", "Test User", cwd=repo)
    run("config", "user.email", "test@example.com", cwd=repo)
    run("checkout", "-b", "main", cwd=repo)
    (repo / "Cargo.toml").write_text('[package]\nname = "dockrev"\nversion = "0.1.0"\n')
    (repo / "README.md").write_text("base\n")
    run("add", "Cargo.toml", "README.md", cwd=repo)
    run("commit", "-m", "base", cwd=repo)
    run("tag", "0.1.0", cwd=repo)

    (repo / "README.md").write_text("patch one\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "patch one", cwd=repo)
    first_sha = run("rev-parse", "HEAD", cwd=repo)

    (repo / "README.md").write_text("patch two\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "patch two", cwd=repo)
    target_sha = run("rev-parse", "HEAD", cwd=repo)

    original_cwd = Path.cwd()
    original_load_pr = module.load_pr_for_commit
    original_git = module.git

    os.chdir(repo)
    try:
        def fake_load_pr(api_root, repository, token, commit_sha, **kwargs):
            return {
                first_sha: make_pr(401, "First patch", first_sha, ["type:patch", "channel:stable"]),
                target_sha: make_pr(402, "Second patch", target_sha, ["type:patch", "channel:stable"]),
            }.get(commit_sha)

        module.load_pr_for_commit = fake_load_pr
        module.git = fake_push_git(original_git, module.DEFAULT_NOTES_REF)
        exit_code = module.ensure_snapshot(
            argparse.Namespace(
                target_sha=target_sha,
                github_repository="IvanLi-CN/dockrev",
                github_token="token",
                notes_ref=module.DEFAULT_NOTES_REF,
                registry="ghcr.io",
                api_root="https://api.github.com",
                output=str(repo / "empty-notes.json"),
                max_attempts=1,
                target_only=False,
            )
        )
        assert exit_code == 0
        first_snapshot = module.read_snapshot(module.DEFAULT_NOTES_REF, first_sha)
        target_snapshot = module.read_snapshot(module.DEFAULT_NOTES_REF, target_sha)
        assert first_snapshot is not None
        assert first_snapshot["next_stable_version"] == "0.1.1"
        assert target_snapshot is not None
        assert target_snapshot["base_stable_version"] == "0.1.1"
        assert target_snapshot["next_stable_version"] == "0.1.2"
    finally:
        module.load_pr_for_commit = original_load_pr
        module.git = original_git
        os.chdir(original_cwd)


with tempfile.TemporaryDirectory(prefix="release-snapshot-catch-up-") as tmp:
    repo = Path(tmp)
    run("init", cwd=repo)
    run("config", "user.name", "Test User", cwd=repo)
    run("config", "user.email", "test@example.com", cwd=repo)
    run("checkout", "-b", "main", cwd=repo)
    (repo / "Cargo.toml").write_text('[package]\nname = "dockrev"\nversion = "0.1.0"\n')
    (repo / "README.md").write_text("base\n")
    run("add", "Cargo.toml", "README.md", cwd=repo)
    run("commit", "-m", "base", cwd=repo)
    run("tag", "0.1.0", cwd=repo)

    (repo / "README.md").write_text("legacy unlabeled\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "legacy unlabeled", cwd=repo)
    legacy_sha = run("rev-parse", "HEAD", cwd=repo)

    (repo / "README.md").write_text("existing snapshot\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "existing snapshot", cwd=repo)
    snap_sha = run("rev-parse", "HEAD", cwd=repo)

    (repo / "README.md").write_text("mid pending\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "mid pending", cwd=repo)
    mid_sha = run("rev-parse", "HEAD", cwd=repo)

    (repo / "README.md").write_text("target pending\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "target pending", cwd=repo)
    target_sha = run("rev-parse", "HEAD", cwd=repo)

    existing_snapshot = {
        "schema_version": module.SNAPSHOT_SCHEMA_VERSION,
        "target_sha": snap_sha,
        "pr_number": 301,
        "pr_title": "Existing snapshot",
        "registry": "ghcr.io",
        "pr_head_sha": snap_sha,
        "type_label": "type:patch",
        "channel_label": "channel:stable",
        "release_bump": "patch",
        "release_channel": "stable",
        "release_enabled": True,
        "release_prerelease": False,
        "publish_latest": True,
        "image_name_lower": "ivanli-cn/dockrev",
        "supervisor_image_name_lower": "ivanli-cn/dockrev-supervisor",
        "base_stable_version": "0.1.0",
        "next_stable_version": "0.1.1",
        "app_effective_version": "0.1.1",
        "release_tag": "0.1.1",
        "tags_csv": "ghcr.io/ivanli-cn/dockrev:0.1.1,ghcr.io/ivanli-cn/dockrev:latest",
        "supervisor_tags_csv": "ghcr.io/ivanli-cn/dockrev-supervisor:0.1.1,ghcr.io/ivanli-cn/dockrev-supervisor:latest",
        "notes_ref": module.DEFAULT_NOTES_REF,
        "snapshot_source": "ci-main",
        "created_at": "2026-03-15T00:00:00Z",
    }
    run("notes", f"--ref={module.DEFAULT_NOTES_REF}", "add", "-f", "-m", json.dumps(existing_snapshot), snap_sha, cwd=repo)

    original_cwd = Path.cwd()
    original_load_pr = module.load_pr_for_commit
    original_build_snapshot = module.build_snapshot
    original_git = module.git
    calls: list[str] = []

    def fake_build_snapshot(*, target_sha: str, **kwargs: object):
        calls.append(target_sha)
        version_map = {
            mid_sha: ("0.1.1", "0.1.2", "0.1.2"),
            target_sha: ("0.1.2", "0.1.3", "0.1.3"),
        }
        if target_sha not in version_map:
            raise AssertionError(f"unexpected snapshot build for {target_sha}")
        base_version, next_version, release_tag = version_map[target_sha]
        return {
            "schema_version": module.SNAPSHOT_SCHEMA_VERSION,
            "target_sha": target_sha,
            "pr_number": 302 if target_sha == mid_sha else 303,
            "pr_title": "Pending snapshot",
            "registry": "ghcr.io",
            "pr_head_sha": target_sha,
            "type_label": "type:patch",
            "channel_label": "channel:stable",
            "release_bump": "patch",
            "release_channel": "stable",
            "release_enabled": True,
            "release_prerelease": False,
            "publish_latest": True,
            "image_name_lower": "ivanli-cn/dockrev",
            "supervisor_image_name_lower": "ivanli-cn/dockrev-supervisor",
            "base_stable_version": base_version,
            "next_stable_version": next_version,
            "app_effective_version": next_version,
            "release_tag": release_tag,
            "tags_csv": f"ghcr.io/ivanli-cn/dockrev:{release_tag},ghcr.io/ivanli-cn/dockrev:latest",
            "supervisor_tags_csv": f"ghcr.io/ivanli-cn/dockrev-supervisor:{release_tag},ghcr.io/ivanli-cn/dockrev-supervisor:latest",
            "notes_ref": module.DEFAULT_NOTES_REF,
            "snapshot_source": "ci-main",
            "created_at": "2026-03-15T00:00:00Z",
        }

    os.chdir(repo)
    try:
        def fake_load_pr(api_root, repository, token, commit_sha, **kwargs):
            return {
                mid_sha: make_pr(302, "Mid pending", mid_sha, ["type:patch", "channel:stable"]),
                target_sha: make_pr(303, "Target pending", target_sha, ["type:patch", "channel:stable"]),
            }.get(commit_sha)

        module.load_pr_for_commit = fake_load_pr
        module.build_snapshot = fake_build_snapshot
        module.git = fake_push_git(original_git, module.DEFAULT_NOTES_REF)
        exit_code = module.ensure_snapshot(
            argparse.Namespace(
                target_sha=target_sha,
                github_repository="IvanLi-CN/dockrev",
                github_token="token",
                notes_ref=module.DEFAULT_NOTES_REF,
                registry="ghcr.io",
                api_root="https://api.github.com",
                output=str(repo / "catch-up.json"),
                max_attempts=1,
                target_only=False,
            )
        )
        assert exit_code == 0
        assert calls == [mid_sha, target_sha]
        assert module.read_snapshot(module.DEFAULT_NOTES_REF, legacy_sha) is None
        assert module.read_snapshot(module.DEFAULT_NOTES_REF, snap_sha) is not None
        assert module.read_snapshot(module.DEFAULT_NOTES_REF, mid_sha) is not None
        stored = module.read_snapshot(module.DEFAULT_NOTES_REF, target_sha)
        assert stored is not None
        assert stored["release_tag"] == "0.1.3"
    finally:
        module.load_pr_for_commit = original_load_pr
        module.build_snapshot = original_build_snapshot
        module.git = original_git
        os.chdir(original_cwd)


with tempfile.TemporaryDirectory(prefix="release-snapshot-overrides-") as tmp:
    repo = Path(tmp)
    run("init", cwd=repo)
    run("config", "user.name", "Test User", cwd=repo)
    run("config", "user.email", "test@example.com", cwd=repo)
    run("checkout", "-b", "main", cwd=repo)
    (repo / "Cargo.toml").write_text('[package]\nname = "dockrev"\nversion = "0.1.0"\n')
    (repo / "README.md").write_text("base\n")
    run("add", "Cargo.toml", "README.md", cwd=repo)
    run("commit", "-m", "base", cwd=repo)
    run("tag", "0.1.0", cwd=repo)

    (repo / "README.md").write_text("frozen target\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "frozen target", cwd=repo)
    target_sha = run("rev-parse", "HEAD", cwd=repo)

    original_cwd = Path.cwd()
    original_loader = module.load_pr_for_commit
    original_git = module.git
    try:
        os.chdir(repo)
        module.load_pr_for_commit = lambda api_root, repository, token, commit_sha, **kwargs: {
            target_sha: make_pr(601, "Frozen target", target_sha, ["type:patch", "channel:stable"]),
        }[commit_sha]
        snapshot = module.build_snapshot(
            target_sha=target_sha,
            repository="IvanLi-CN/dockrev",
            token="token",
            notes_ref=module.DEFAULT_NOTES_REF,
            registry="ghcr.io",
            api_root="https://api.github.com",
        )
        run("notes", f"--ref={module.DEFAULT_NOTES_REF}", "add", "-f", "-m", json.dumps(snapshot), target_sha, cwd=repo)

        module.git = fake_push_git(original_git, module.DEFAULT_OVERRIDE_NOTES_REF)
        exit_code = module.record_override(
            argparse.Namespace(
                target_sha=target_sha,
                snapshot_notes_ref=module.DEFAULT_NOTES_REF,
                publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
                override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
                status="skip",
                reason="release-infra mislabel under no-extra-credential model",
                output=str(repo / "override.json"),
                max_attempts=1,
            )
        )
        assert exit_code == 0
        override = module.read_override(module.DEFAULT_OVERRIDE_NOTES_REF, target_sha)
        assert override is not None
        assert override["status"] == "skip"
        assert override["reason"] == "release-infra mislabel under no-extra-credential model"
        assert (
            module.release_state_for_target(
                snapshot,
                publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
                override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
            )
            == "skipped"
        )
        pending = module.pending_release_targets(
            module.DEFAULT_NOTES_REF,
            target_sha,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
        )
        assert pending == []
    finally:
        module.load_pr_for_commit = original_loader
        module.git = original_git
        os.chdir(original_cwd)


with tempfile.TemporaryDirectory(prefix="release-snapshot-publication-regression-") as tmp:
    repo = Path(tmp)
    run("init", cwd=repo)
    run("config", "user.name", "Test User", cwd=repo)
    run("config", "user.email", "test@example.com", cwd=repo)
    run("checkout", "-b", "main", cwd=repo)
    (repo / "Cargo.toml").write_text('[package]\nname = "dockrev"\nversion = "0.35.7"\n')
    (repo / "README.md").write_text("base\n")
    run("add", "Cargo.toml", "README.md", cwd=repo)
    run("commit", "-m", "base", cwd=repo)
    run("tag", "0.35.7", cwd=repo)

    (repo / "README.md").write_text("0.35.8 pending\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "0.35.8 pending", cwd=repo)
    old_sha = run("rev-parse", "HEAD", cwd=repo)

    (repo / "README.md").write_text("0.35.9 pending\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "0.35.9 pending", cwd=repo)
    new_sha = run("rev-parse", "HEAD", cwd=repo)

    original_cwd = Path.cwd()
    original_loader = module.load_pr_for_commit
    original_git = module.git
    try:
        os.chdir(repo)
        module.load_pr_for_commit = lambda api_root, repository, token, target_sha, **kwargs: {
            old_sha: make_pr(501, "Release 0.35.8", old_sha, ["type:patch", "channel:stable"]),
            new_sha: make_pr(502, "Release 0.35.9", new_sha, ["type:patch", "channel:stable"]),
        }[target_sha]

        old_snapshot = module.build_snapshot(
            target_sha=old_sha,
            repository="IvanLi-CN/dockrev",
            token="token",
            notes_ref=module.DEFAULT_NOTES_REF,
            registry="ghcr.io",
            api_root="https://api.github.com",
        )
        assert old_snapshot["release_tag"] == "0.35.8"
        run("notes", f"--ref={module.DEFAULT_NOTES_REF}", "add", "-f", "-m", json.dumps(old_snapshot), old_sha, cwd=repo)

        new_snapshot = module.build_snapshot(
            target_sha=new_sha,
            repository="IvanLi-CN/dockrev",
            token="token",
            notes_ref=module.DEFAULT_NOTES_REF,
            registry="ghcr.io",
            api_root="https://api.github.com",
        )
        assert new_snapshot["release_tag"] == "0.35.9"
        run("notes", f"--ref={module.DEFAULT_NOTES_REF}", "add", "-f", "-m", json.dumps(new_snapshot), new_sha, cwd=repo)

        exported = dict(old_snapshot)
        exported["publish_latest"] = module.publish_latest_for_snapshot(
            old_snapshot,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            main_ref=new_sha,
        )
        exported["tags_csv"] = module.publication_tags(
            old_snapshot,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            main_ref=new_sha,
        )
        assert exported["publish_latest"] is True
        assert exported["tags_csv"] == "ghcr.io/ivanli-cn/dockrev:0.35.8,ghcr.io/ivanli-cn/dockrev:latest"

        module.git = fake_push_git(original_git, module.DEFAULT_PUBLICATION_NOTES_REF)
        exit_code = module.record_publication(
            argparse.Namespace(
                target_sha=new_sha,
                snapshot_notes_ref=module.DEFAULT_NOTES_REF,
                publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
                override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
                dockrev_digest="sha256:" + ("1" * 64),
                dockrev_supervisor_digest="sha256:" + ("2" * 64),
                published_at="2026-03-27T10:20:30Z",
                output=str(repo / "publication.json"),
                max_attempts=1,
            )
        )
        assert exit_code == 0
        publication = module.read_publication(module.DEFAULT_PUBLICATION_NOTES_REF, new_sha)
        assert publication is not None
        assert publication["release_tag"] == "0.35.9"
        assert publication["release_channel"] == "stable"
        assert publication["published_at"] == "2026-03-27T10:20:30Z"

        exported_after = dict(old_snapshot)
        exported_after["publish_latest"] = module.publish_latest_for_snapshot(
            old_snapshot,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            main_ref=new_sha,
        )
        exported_after["tags_csv"] = module.publication_tags(
            old_snapshot,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            main_ref=new_sha,
        )
        assert exported_after["publish_latest"] is False
        assert exported_after["tags_csv"] == "ghcr.io/ivanli-cn/dockrev:0.35.8"
    finally:
        module.load_pr_for_commit = original_loader
        module.git = original_git
        os.chdir(original_cwd)


with tempfile.TemporaryDirectory(prefix="release-snapshot-tag-only-state-regression-") as tmp:
    repo = Path(tmp)
    run("init", cwd=repo)
    run("config", "user.name", "Test User", cwd=repo)
    run("config", "user.email", "test@example.com", cwd=repo)
    run("checkout", "-b", "main", cwd=repo)
    (repo / "Cargo.toml").write_text('[package]\nname = "dockrev"\nversion = "0.38.0"\n')
    (repo / "README.md").write_text("base\n")
    run("add", "Cargo.toml", "README.md", cwd=repo)
    run("commit", "-m", "base", cwd=repo)
    run("tag", "0.38.0", cwd=repo)

    (repo / "README.md").write_text("published release\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "published release", cwd=repo)
    published_sha = run("rev-parse", "HEAD", cwd=repo)

    (repo / "README.md").write_text("tag-only pending release\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "tag-only pending release", cwd=repo)
    tagged_pending_sha = run("rev-parse", "HEAD", cwd=repo)

    original_cwd = Path.cwd()
    original_loader = module.load_pr_for_commit
    original_git = module.git
    try:
        os.chdir(repo)
        module.load_pr_for_commit = lambda api_root, repository, token, target_sha, **kwargs: {
            published_sha: make_pr(601, "Release 0.38.1", published_sha, ["type:patch", "channel:stable"]),
            tagged_pending_sha: make_pr(602, "Release 0.38.2", tagged_pending_sha, ["type:patch", "channel:stable"]),
        }[target_sha]

        published_snapshot = module.build_snapshot(
            target_sha=published_sha,
            repository="IvanLi-CN/dockrev",
            token="token",
            notes_ref=module.DEFAULT_NOTES_REF,
            registry="ghcr.io",
            api_root="https://api.github.com",
        )
        assert published_snapshot["release_tag"] == "0.38.1"
        run("notes", f"--ref={module.DEFAULT_NOTES_REF}", "add", "-f", "-m", json.dumps(published_snapshot), published_sha, cwd=repo)

        module.git = fake_push_git(original_git, module.DEFAULT_PUBLICATION_NOTES_REF)
        exit_code = module.record_publication(
            argparse.Namespace(
                target_sha=published_sha,
                snapshot_notes_ref=module.DEFAULT_NOTES_REF,
                publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
                override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
                dockrev_digest="sha256:" + ("3" * 64),
                dockrev_supervisor_digest="sha256:" + ("4" * 64),
                published_at="2026-04-03T08:00:00Z",
                output=str(repo / "published-publication.json"),
                max_attempts=1,
            )
        )
        assert exit_code == 0
        assert (
            module.release_state_for_target(
                published_snapshot,
                publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
                override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
            )
            == "published"
        )

        tagged_pending_snapshot = module.build_snapshot(
            target_sha=tagged_pending_sha,
            repository="IvanLi-CN/dockrev",
            token="token",
            notes_ref=module.DEFAULT_NOTES_REF,
            registry="ghcr.io",
            api_root="https://api.github.com",
        )
        assert tagged_pending_snapshot["release_tag"] == "0.38.2"
        run(
            "notes",
            f"--ref={module.DEFAULT_NOTES_REF}",
            "add",
            "-f",
            "-m",
            json.dumps(tagged_pending_snapshot),
            tagged_pending_sha,
            cwd=repo,
        )
        run("tag", "-a", tagged_pending_snapshot["release_tag"], "-m", "Release 0.38.2", tagged_pending_sha, cwd=repo)

        assert module.release_tag_points_to_target(tagged_pending_snapshot) is True
        assert (
            module.release_state_for_target(
                tagged_pending_snapshot,
                publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
                override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
            )
            == "pending"
        )
        pending = module.pending_release_targets(
            module.DEFAULT_NOTES_REF,
            tagged_pending_sha,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
        )
        assert pending == [tagged_pending_sha], pending

        module.git = fake_push_git(original_git, module.DEFAULT_OVERRIDE_NOTES_REF)
        exit_code = module.record_override(
            argparse.Namespace(
                target_sha=tagged_pending_sha,
                snapshot_notes_ref=module.DEFAULT_NOTES_REF,
                publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
                override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
                status="skip",
                reason="manual backfill skipped after verification",
                output=str(repo / "tagged-pending-override.json"),
                max_attempts=1,
            )
        )
        assert exit_code == 0
        assert (
            module.release_state_for_target(
                tagged_pending_snapshot,
                publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
                override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
            )
            == "skipped"
        )
        pending_after_override = module.pending_release_targets(
            module.DEFAULT_NOTES_REF,
            tagged_pending_sha,
            publication_notes_ref=module.DEFAULT_PUBLICATION_NOTES_REF,
            override_notes_ref=module.DEFAULT_OVERRIDE_NOTES_REF,
        )
        assert pending_after_override == []
    finally:
        module.load_pr_for_commit = original_loader
        module.git = original_git
        os.chdir(original_cwd)

comments: list[dict[str, object]] = []
requests: list[dict[str, object]] = []
original_comment_request = comment_module.github_request_json


def fake_comment_request(api_root, token, method, path, *, body=None, query=None):
    requests.append({"method": method, "path": path, "body": body, "query": query})
    if method == "GET":
        return [dict(comment) for comment in comments]
    if method == "POST":
        created = {
            "id": 900 + len(comments) + 1,
            "body": body["body"],
            "user": {"login": comment_module.BOT_LOGIN},
        }
        comments.append(created)
        return created
    if method == "PATCH":
        comment_id = int(path.rsplit("/", 1)[1])
        for index, comment in enumerate(comments):
            if comment["id"] == comment_id:
                updated = dict(comment)
                updated["body"] = body["body"]
                comments[index] = updated
                return updated
        raise AssertionError(f"missing comment id {comment_id}")
    if method == "DELETE":
        comment_id = int(path.rsplit("/", 1)[1])
        for index, comment in enumerate(comments):
            if comment["id"] == comment_id:
                comments.pop(index)
                return None
        raise AssertionError(f"missing comment id {comment_id}")
    raise AssertionError(f"unexpected method {method}")


comment_module.github_request_json = fake_comment_request
try:
    created = comment_module.upsert_release_comment(
        api_root="https://api.github.com",
        repository="IvanLi-CN/dockrev",
        token="token",
        pr_number=186,
        release_tag="0.35.9",
        release_channel="stable",
        release_url="https://github.com/IvanLi-CN/dockrev/releases/tag/0.35.9",
        workflow_run_url="https://github.com/IvanLi-CN/dockrev/actions/runs/179",
    )
    assert created["comment_status"] == "create"
    assert requests[0]["method"] == "GET"
    assert requests[1]["method"] == "POST"
    assert comment_module.COMMENT_MARKER in comments[0]["body"]
    assert "Version: `0.35.9`" in comments[0]["body"]
    assert "Channel: `stable`" in comments[0]["body"]

    requests.clear()
    updated = comment_module.upsert_release_comment(
        api_root="https://api.github.com",
        repository="IvanLi-CN/dockrev",
        token="token",
        pr_number=186,
        release_tag="0.36.0-rc.abcdef0",
        release_channel="rc",
        release_url="https://github.com/IvanLi-CN/dockrev/releases/tag/0.36.0-rc.abcdef0",
        workflow_run_url="https://github.com/IvanLi-CN/dockrev/actions/runs/180",
    )
    assert updated["comment_status"] == "update"
    assert requests[0]["method"] == "GET"
    assert requests[1]["method"] == "PATCH"
    assert "Version: `0.36.0-rc.abcdef0`" in comments[0]["body"]
    assert "Channel: `rc`" in comments[0]["body"]

    comments[:] = [
        {
            "id": 771,
            "body": f"{comment_module.COMMENT_MARKER}\nold duplicate",
            "user": {"login": comment_module.BOT_LOGIN},
        },
        {
            "id": 772,
            "body": f"{comment_module.COMMENT_MARKER}\nnewest duplicate",
            "user": {"login": comment_module.BOT_LOGIN},
        },
    ]
    requests.clear()
    deduped = comment_module.upsert_release_comment(
        api_root="https://api.github.com",
        repository="IvanLi-CN/dockrev",
        token="token",
        pr_number=186,
        release_tag="0.36.1",
        release_channel="stable",
        release_url="https://github.com/IvanLi-CN/dockrev/releases/tag/0.36.1",
        workflow_run_url="https://github.com/IvanLi-CN/dockrev/actions/runs/181",
    )
    assert deduped["comment_status"] == "update"
    assert [request["method"] for request in requests[:3]] == ["GET", "PATCH", "GET"]
    assert any(request["method"] == "DELETE" and request["path"].endswith("/771") for request in requests)
    assert len(comments) == 1
    assert comments[0]["id"] == 772
    assert "Version: `0.36.1`" in comments[0]["body"]

    comments[:] = [
        {
            "id": 777,
            "body": f"{comment_module.COMMENT_MARKER}\nforeign marker",
            "user": {"login": "octocat"},
        }
    ]
    requests.clear()
    try:
        comment_module.upsert_release_comment(
            api_root="https://api.github.com",
            repository="IvanLi-CN/dockrev",
            token="token",
            pr_number=186,
            release_tag="0.35.9",
            release_channel="stable",
            release_url="https://github.com/IvanLi-CN/dockrev/releases/tag/0.35.9",
            workflow_run_url="https://github.com/IvanLi-CN/dockrev/actions/runs/179",
        )
    except comment_module.CommentError as exc:
        assert "cannot satisfy release comment contract" in str(exc)
    else:
        raise AssertionError("expected foreign marker contract failure")
    assert len(requests) == 1
    assert requests[0]["method"] == "GET"
finally:
    comment_module.github_request_json = original_comment_request

print("release_snapshot.py + release_pr_comment.py self-test: ok")
PY
