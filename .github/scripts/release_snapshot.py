#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib import error, parse, request

SNAPSHOT_SCHEMA_VERSION = 1
PUBLICATION_SCHEMA_VERSION = 1
DEFAULT_NOTES_REF = "refs/notes/release-snapshots"
DEFAULT_PUBLICATION_NOTES_REF = "refs/notes/release-publications"
ALLOWED_SNAPSHOT_SOURCES = {"ci-main", "manual-backfill", "pr-intent-artifact", "legacy-pr-labels"}
ALLOWED_TYPE_LABELS = {
    "type:patch",
    "type:minor",
    "type:major",
    "type:docs",
    "type:skip",
}
ALLOWED_CHANNEL_LABELS = {"channel:stable", "channel:rc"}
STABLE_VERSION_RE = re.compile(r"^(?:v)?(\d+)\.(\d+)\.(\d+)$")
STABLE_TAG_RE = re.compile(r"^v?\d+\.\d+\.\d+$")
RELEASE_TAG_RE = re.compile(r"^v?\d+\.\d+\.\d+(?:-rc\.[0-9a-f]{7})?$")
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")


class SnapshotError(RuntimeError):
    pass


@dataclass(frozen=True, order=True)
class StableVersion:
    major: int
    minor: int
    patch: int

    @classmethod
    def parse(cls, value: str) -> "StableVersion":
        match = STABLE_VERSION_RE.fullmatch(value)
        if not match:
            raise SnapshotError(f"Invalid stable version: {value}")
        return cls(*(int(part) for part in match.groups()))

    @classmethod
    def from_tag(cls, tag: str) -> "StableVersion | None":
        match = STABLE_VERSION_RE.fullmatch(tag)
        if not match:
            return None
        return cls(*(int(part) for part in match.groups()))

    def bump(self, bump: str) -> "StableVersion":
        if bump == "patch":
            return StableVersion(self.major, self.minor, self.patch + 1)
        if bump == "minor":
            return StableVersion(self.major, self.minor + 1, 0)
        if bump == "major":
            return StableVersion(self.major + 1, 0, 0)
        raise SnapshotError(f"Unknown release bump: {bump}")

    def render(self) -> str:
        return f"{self.major}.{self.minor}.{self.patch}"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Manage immutable release snapshots stored in git notes.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    ensure = subparsers.add_parser("ensure", help="Create or reuse the immutable snapshot for a main commit.")
    ensure.add_argument("--target-sha", required=True)
    ensure.add_argument("--github-repository", required=True)
    ensure.add_argument("--github-token", required=True)
    ensure.add_argument("--notes-ref", default=DEFAULT_NOTES_REF)
    ensure.add_argument("--registry", default="ghcr.io")
    ensure.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    ensure.add_argument("--output", required=True)
    ensure.add_argument("--max-attempts", type=int, default=6)
    ensure.add_argument(
        "--target-only",
        action="store_true",
        help="Only materialize the requested target commit instead of filling every missing first-parent snapshot on the path.",
    )

    export_cmd = subparsers.add_parser("export", help="Export a stored release snapshot into GitHub outputs.")
    export_cmd.add_argument("--target-sha", required=True)
    export_cmd.add_argument("--notes-ref", default=DEFAULT_NOTES_REF)
    export_cmd.add_argument("--main-ref", default="")
    export_cmd.add_argument(
        "--resolve-publication-tags",
        action="store_true",
        help="Re-resolve stable manifest tags from the publication ledger so superseded releases stop updating latest.",
    )
    export_cmd.add_argument("--publication-notes-ref", default=DEFAULT_PUBLICATION_NOTES_REF)
    export_cmd.add_argument("--github-output", default=os.environ.get("GITHUB_OUTPUT", ""))

    next_pending = subparsers.add_parser(
        "next-pending",
        help="Find the oldest unreleased snapshot on the first-parent path up to a given main commit.",
    )
    next_pending.add_argument("--notes-ref", default=DEFAULT_NOTES_REF)
    next_pending.add_argument("--main-ref", required=True)
    next_pending.add_argument("--upper-bound", default="")
    next_pending.add_argument("--github-output", default=os.environ.get("GITHUB_OUTPUT", ""))

    record_publication = subparsers.add_parser(
        "record-publication",
        help="Record a mutable publication ledger note after GHCR images have been pushed.",
    )
    record_publication.add_argument("--target-sha", required=True)
    record_publication.add_argument("--snapshot-notes-ref", default=DEFAULT_NOTES_REF)
    record_publication.add_argument("--publication-notes-ref", default=DEFAULT_PUBLICATION_NOTES_REF)
    record_publication.add_argument("--dockrev-digest", required=True)
    record_publication.add_argument("--dockrev-supervisor-digest", required=True)
    record_publication.add_argument("--published-at", default="")
    record_publication.add_argument("--output", default="")
    record_publication.add_argument("--max-attempts", type=int, default=3)

    return parser.parse_args()


def git(*args: str, check: bool = True, capture_output: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        ["git", *args],
        check=False,
        text=True,
        capture_output=capture_output,
    )
    if check and result.returncode != 0:
        stderr = result.stderr.strip()
        stdout = result.stdout.strip()
        detail = stderr or stdout or f"git {' '.join(args)} failed"
        raise SnapshotError(detail)
    return result


def git_output(*args: str) -> str:
    return git(*args).stdout.strip()


def normalize_sha(target_sha: str) -> str:
    if not SHA_RE.fullmatch(target_sha):
        raise SnapshotError(f"Invalid target SHA: {target_sha}")
    git("cat-file", "-e", f"{target_sha}^{{commit}}")
    return target_sha


def read_json_note(notes_ref: str, target_sha: str) -> Any | None:
    result = git("notes", f"--ref={notes_ref}", "show", target_sha, check=False)
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise SnapshotError(f"Git note in {notes_ref} for {target_sha} is not valid JSON") from exc


def read_snapshot(notes_ref: str, target_sha: str) -> dict[str, Any] | None:
    payload = read_json_note(notes_ref, target_sha)
    if payload is None:
        return None
    return validate_snapshot(payload, expected_sha=target_sha)


def validate_snapshot(payload: Any, *, expected_sha: str | None = None) -> dict[str, Any]:
    if not isinstance(payload, dict):
        raise SnapshotError("Release snapshot note must decode to an object")
    if payload.get("schema_version") != SNAPSHOT_SCHEMA_VERSION:
        raise SnapshotError(f"Unsupported release snapshot schema: {payload.get('schema_version')!r}")

    target_sha = payload.get("target_sha")
    if not isinstance(target_sha, str) or not SHA_RE.fullmatch(target_sha):
        raise SnapshotError("Release snapshot target_sha must be a 40-char commit SHA")
    if expected_sha and target_sha != expected_sha:
        raise SnapshotError(f"Release snapshot target_sha mismatch: expected {expected_sha}, got {target_sha}")

    strings_required = [
        "type_label",
        "channel_label",
        "release_bump",
        "release_channel",
        "image_name_lower",
        "supervisor_image_name_lower",
        "snapshot_source",
        "registry",
    ]
    for key in strings_required:
        value = payload.get(key)
        if not isinstance(value, str) or not value:
            raise SnapshotError(f"Release snapshot {key} must be a non-empty string")

    if payload["type_label"] not in ALLOWED_TYPE_LABELS:
        raise SnapshotError(f"Unknown type label in release snapshot: {payload['type_label']}")
    if payload["channel_label"] not in ALLOWED_CHANNEL_LABELS:
        raise SnapshotError(f"Unknown channel label in release snapshot: {payload['channel_label']}")
    if payload["snapshot_source"] not in ALLOWED_SNAPSHOT_SOURCES:
        raise SnapshotError(
            f"Release snapshot snapshot_source must be one of {', '.join(sorted(ALLOWED_SNAPSHOT_SOURCES))}"
        )

    if not isinstance(payload.get("release_enabled"), bool):
        raise SnapshotError("Release snapshot release_enabled must be boolean")
    if not isinstance(payload.get("release_prerelease"), bool):
        raise SnapshotError("Release snapshot release_prerelease must be boolean")
    if not isinstance(payload.get("publish_latest"), bool):
        raise SnapshotError("Release snapshot publish_latest must be boolean")

    pr_number = payload.get("pr_number")
    if pr_number is not None and not isinstance(pr_number, int):
        raise SnapshotError("Release snapshot pr_number must be an integer or null")
    for key in ("pr_title", "notes_ref", "tags_csv", "supervisor_tags_csv"):
        value = payload.get(key)
        if value is not None and not isinstance(value, str):
            raise SnapshotError(f"Release snapshot {key} must be a string or null")
    pr_head_sha = payload.get("pr_head_sha")
    if pr_head_sha not in (None, "") and (not isinstance(pr_head_sha, str) or not SHA_RE.fullmatch(pr_head_sha)):
        raise SnapshotError("Release snapshot pr_head_sha must be a 40-char commit SHA when present")

    if payload["release_enabled"]:
        for key in ("base_stable_version", "next_stable_version", "app_effective_version", "release_tag"):
            value = payload.get(key)
            if not isinstance(value, str) or not value:
                raise SnapshotError(f"Release snapshot {key} must be a non-empty string when release_enabled=true")

        StableVersion.parse(payload["base_stable_version"])
        StableVersion.parse(payload["next_stable_version"])
        if not isinstance(payload["release_tag"], str) or not RELEASE_TAG_RE.fullmatch(payload["release_tag"]):
            raise SnapshotError("Release snapshot release_tag is malformed")

        if payload["release_channel"] == "stable":
            if payload["release_prerelease"]:
                raise SnapshotError("Stable release snapshot must not be marked prerelease")
            if payload["release_tag"] != payload["app_effective_version"]:
                raise SnapshotError("Stable release_tag must equal app_effective_version")
        elif payload["release_channel"] == "rc":
            if not payload["release_prerelease"]:
                raise SnapshotError("RC release snapshot must be marked prerelease")
            if "-rc." not in payload["release_tag"]:
                raise SnapshotError("RC release_tag must contain -rc.<sha7>")
        else:
            raise SnapshotError(f"Unknown release_channel in snapshot: {payload['release_channel']}")
    else:
        for key in (
            "base_stable_version",
            "next_stable_version",
            "app_effective_version",
            "release_tag",
            "tags_csv",
            "supervisor_tags_csv",
        ):
            if payload.get(key) not in (None, ""):
                raise SnapshotError(f"Release snapshot {key} must be empty when release_enabled=false")
        if payload["publish_latest"]:
            raise SnapshotError("Disabled release snapshot must not publish latest")

    return payload


def validate_digest(value: Any, *, field_name: str) -> str:
    if not isinstance(value, str) or not DIGEST_RE.fullmatch(value):
        raise SnapshotError(f"Release publication {field_name} must be a sha256 digest")
    return value


def validate_publication(payload: Any, *, expected_sha: str | None = None) -> dict[str, Any]:
    if not isinstance(payload, dict):
        raise SnapshotError("Release publication note must decode to an object")
    if payload.get("schema_version") != PUBLICATION_SCHEMA_VERSION:
        raise SnapshotError(f"Unsupported release publication schema: {payload.get('schema_version')!r}")

    target_sha = payload.get("target_sha")
    if not isinstance(target_sha, str) or not SHA_RE.fullmatch(target_sha):
        raise SnapshotError("Release publication target_sha must be a 40-char commit SHA")
    if expected_sha and target_sha != expected_sha:
        raise SnapshotError(f"Release publication target_sha mismatch: expected {expected_sha}, got {target_sha}")

    pr_number = payload.get("pr_number")
    if not isinstance(pr_number, int) or pr_number <= 0:
        raise SnapshotError("Release publication pr_number must be a positive integer")

    release_tag = payload.get("release_tag")
    if not isinstance(release_tag, str) or not RELEASE_TAG_RE.fullmatch(release_tag):
        raise SnapshotError("Release publication release_tag is malformed")

    release_channel = payload.get("release_channel")
    if release_channel not in {"stable", "rc"}:
        raise SnapshotError(f"Unknown release_channel in publication: {release_channel!r}")

    published_at = payload.get("published_at")
    if not isinstance(published_at, str) or not published_at:
        raise SnapshotError("Release publication published_at must be a non-empty string")

    validate_digest(payload.get("dockrev_digest"), field_name="dockrev_digest")
    validate_digest(payload.get("dockrev_supervisor_digest"), field_name="dockrev_supervisor_digest")
    return payload


def read_publication(notes_ref: str, target_sha: str) -> dict[str, Any] | None:
    payload = read_json_note(notes_ref, target_sha)
    if payload is None:
        return None
    return validate_publication(payload, expected_sha=target_sha)


def fetch_notes_ref(notes_ref: str) -> None:
    probe = git("ls-remote", "--exit-code", "origin", notes_ref, check=False)
    if probe.returncode != 0:
        return
    git("fetch", "--no-tags", "origin", f"+{notes_ref}:{notes_ref}")


def fetch_tags() -> None:
    git("fetch", "--tags", "origin")


def github_request_json(api_root: str, token: str, path: str, query: dict[str, Any] | None = None) -> Any:
    url = f"{api_root.rstrip('/')}{path}"
    if query:
        url += "?" + parse.urlencode(query)
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json, application/vnd.github.groot-preview+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "dockrev-release-snapshot",
    }
    req = request.Request(url, headers=headers)
    try:
        with request.urlopen(req) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        raise SnapshotError(f"GitHub API error on {path}: {exc.code} {body}") from exc
    except error.URLError as exc:
        raise SnapshotError(f"GitHub API request failed on {path}: {exc}") from exc


def load_pr_for_commit(
    api_root: str,
    repository: str,
    token: str,
    target_sha: str,
    *,
    allow_zero: bool = False,
) -> dict[str, Any] | None:
    owner, repo = repository.split("/", 1)
    payload = github_request_json(api_root, token, f"/repos/{owner}/{repo}/commits/{target_sha}/pulls")
    if not isinstance(payload, list):
        raise SnapshotError("GitHub API returned an unexpected payload for commit-associated PRs")
    if len(payload) == 0 and allow_zero:
        return None
    if len(payload) != 1:
        raise SnapshotError(f"Expected exactly 1 PR associated with commit {target_sha}, got {len(payload)}")
    pr_summary = payload[0]
    if not isinstance(pr_summary, dict):
        raise SnapshotError("GitHub API returned a malformed PR payload")
    pr_number = pr_summary.get("number")
    if not isinstance(pr_number, int):
        raise SnapshotError("Commit-associated PR payload is missing a numeric PR number")
    pr = github_request_json(api_root, token, f"/repos/{owner}/{repo}/pulls/{pr_number}")
    if not isinstance(pr, dict):
        raise SnapshotError("GitHub API returned a malformed pull request payload")
    return pr


def current_pr_labels(pr: dict[str, Any]) -> list[str]:
    labels = pr.get("labels")
    if not isinstance(labels, list):
        raise SnapshotError("Pull request payload is missing labels")
    names: list[str] = []
    for label in labels:
        if isinstance(label, str):
            names.append(label)
            continue
        if isinstance(label, dict):
            name = label.get("name")
            if isinstance(name, str):
                names.append(name)
    return sorted(names)


def current_pr_head_sha(pr: dict[str, Any]) -> str:
    head = pr.get("head") or {}
    pr_head_sha = head.get("sha") if isinstance(head, dict) else None
    if not isinstance(pr_head_sha, str) or not SHA_RE.fullmatch(pr_head_sha):
        pr_number = pr.get("number")
        raise SnapshotError(f"Pull request #{pr_number} is missing a valid head.sha")
    return pr_head_sha


def parse_release_labels(labels: list[str]) -> tuple[str, str]:
    type_labels = [label for label in labels if label.startswith("type:")]
    channel_labels = [label for label in labels if label.startswith("channel:")]

    if len(type_labels) != 1:
        raise SnapshotError(
            f"Expected exactly 1 type:* label, got {len(type_labels)}: {', '.join(type_labels) or '(none)'}"
        )
    if len(channel_labels) != 1:
        raise SnapshotError(
            f"Expected exactly 1 channel:* label, got {len(channel_labels)}: {', '.join(channel_labels) or '(none)'}"
        )

    type_label = type_labels[0]
    channel_label = channel_labels[0]
    if type_label not in ALLOWED_TYPE_LABELS:
        raise SnapshotError(f"Unknown type label: {type_label}")
    if channel_label not in ALLOWED_CHANNEL_LABELS:
        raise SnapshotError(f"Unknown channel label: {channel_label}")
    return type_label, channel_label


def cargo_base_version(target_sha: str) -> StableVersion:
    cargo_toml = git_output("show", f"{target_sha}:Cargo.toml")
    match = re.search(r'^version\s*=\s*"(\d+\.\d+\.\d+)"', cargo_toml, re.MULTILINE)
    if not match:
        raise SnapshotError("Failed to detect version from Cargo.toml")
    return StableVersion.parse(match.group(1))


def stable_versions_from_tags(target_sha: str) -> list[StableVersion]:
    tags = git_output("tag", "--merged", target_sha, "-l").splitlines()
    versions: list[StableVersion] = []
    for tag in tags:
        if not STABLE_TAG_RE.fullmatch(tag.strip()):
            continue
        version = StableVersion.from_tag(tag.strip())
        if version is not None:
            versions.append(version)
    return versions


def stable_versions_from_snapshots(notes_ref: str, target_sha: str) -> list[StableVersion]:
    commits = git_output("rev-list", "--first-parent", target_sha).splitlines()
    versions: list[StableVersion] = []
    for commit in commits[1:]:
        snapshot = read_snapshot(notes_ref, commit)
        if not snapshot or not snapshot.get("release_enabled"):
            continue
        if snapshot.get("release_channel") != "stable":
            continue
        next_stable = snapshot.get("next_stable_version")
        if not isinstance(next_stable, str):
            raise SnapshotError(f"Stable snapshot for {commit} is missing next_stable_version")
        versions.append(StableVersion.parse(next_stable))
    return versions


def compute_base_stable_version(notes_ref: str, target_sha: str) -> StableVersion:
    candidates = stable_versions_from_tags(target_sha)
    candidates.extend(stable_versions_from_snapshots(notes_ref, target_sha))
    if not candidates:
        return cargo_base_version(target_sha)
    return max(candidates)


def first_parent_commits(target_sha: str) -> list[str]:
    commits = git_output("rev-list", "--first-parent", "--reverse", target_sha)
    return [commit for commit in commits.splitlines() if commit]


def released_commits_from_tags(target_sha: str) -> set[str]:
    commits: set[str] = set()
    for tag in git_output("tag", "--merged", target_sha, "-l").splitlines():
        tag_name = tag.strip()
        if not tag_name or not RELEASE_TAG_RE.fullmatch(tag_name):
            continue
        commit = git_output("rev-list", "-n", "1", tag_name).strip()
        if commit:
            commits.add(commit)
    return commits


def commits_to_materialize(notes_ref: str, target_sha: str, *, target_only: bool) -> list[str]:
    if target_only:
        return [target_sha]

    commits = first_parent_commits(target_sha)
    tagged_commits = released_commits_from_tags(target_sha)
    anchor_index = -1
    for index, commit in enumerate(commits):
        if commit in tagged_commits:
            anchor_index = index

    earliest_snapshot_after_anchor = -1
    for index in range(anchor_index + 1, len(commits)):
        if read_snapshot(notes_ref, commits[index]) is not None:
            earliest_snapshot_after_anchor = index
            break

    if earliest_snapshot_after_anchor >= 0:
        return commits[earliest_snapshot_after_anchor + 1 :]
    if anchor_index >= 0:
        return commits[anchor_index + 1 :]

    for index, commit in enumerate(commits):
        if read_snapshot(notes_ref, commit) is not None:
            return commits[index + 1 :]
    return [target_sha]


def commits_after_target(main_ref: str, target_sha: str) -> list[str]:
    git("merge-base", "--is-ancestor", target_sha, main_ref)
    commits = git_output("rev-list", "--first-parent", f"{target_sha}..{main_ref}")
    return [commit for commit in commits.splitlines() if commit]


def has_newer_stable_publication(publication_notes_ref: str, main_ref: str, target_sha: str) -> bool:
    for commit in commits_after_target(main_ref, target_sha):
        publication = read_publication(publication_notes_ref, commit)
        if not publication:
            continue
        if publication.get("release_channel") != "stable":
            continue
        return True
    return False


def render_tags_csv(image_name_lower: str, registry: str, release_tag: str, *, publish_latest: bool) -> str:
    image = f"{registry}/{image_name_lower}"
    tags = [f"{image}:{release_tag}"]
    if publish_latest:
        tags.append(f"{image}:latest")
    return ",".join(tags)


def publish_latest_for_snapshot(snapshot: dict[str, Any], *, publication_notes_ref: str, main_ref: str) -> bool:
    if not snapshot.get("release_enabled"):
        return False
    if snapshot.get("release_channel") != "stable":
        return False
    return not has_newer_stable_publication(publication_notes_ref, main_ref, str(snapshot["target_sha"]))


def publication_tags(snapshot: dict[str, Any], *, publication_notes_ref: str, main_ref: str) -> str:
    if not snapshot.get("release_enabled"):
        return ""
    return render_tags_csv(
        str(snapshot["image_name_lower"]),
        str(snapshot["registry"]),
        str(snapshot["release_tag"]),
        publish_latest=publish_latest_for_snapshot(
            snapshot,
            publication_notes_ref=publication_notes_ref,
            main_ref=main_ref,
        ),
    )


def supervisor_publication_tags(snapshot: dict[str, Any], *, publication_notes_ref: str, main_ref: str) -> str:
    if not snapshot.get("release_enabled"):
        return ""
    return render_tags_csv(
        str(snapshot["supervisor_image_name_lower"]),
        str(snapshot["registry"]),
        str(snapshot["release_tag"]),
        publish_latest=publish_latest_for_snapshot(
            snapshot,
            publication_notes_ref=publication_notes_ref,
            main_ref=main_ref,
        ),
    )


def release_tag_points_to_target(snapshot: dict[str, Any]) -> bool:
    if not snapshot.get("release_enabled"):
        return False
    release_tag = snapshot.get("release_tag")
    target_sha = snapshot.get("target_sha")
    if not isinstance(release_tag, str) or not release_tag:
        return False
    if not isinstance(target_sha, str) or not target_sha:
        return False
    result = git("rev-parse", "-q", "--verify", f"refs/tags/{release_tag}", check=False)
    if result.returncode != 0:
        return False
    tagged_sha = git_output("rev-list", "-n", "1", release_tag)
    return tagged_sha == target_sha


def pending_release_targets(notes_ref: str, upper_bound_sha: str) -> list[str]:
    pending: list[str] = []
    for commit in first_parent_commits(upper_bound_sha):
        snapshot = read_snapshot(notes_ref, commit)
        if not snapshot or not snapshot.get("release_enabled"):
            continue
        if release_tag_points_to_target(snapshot):
            continue
        pending.append(commit)
    return pending


def build_snapshot(
    *,
    target_sha: str,
    repository: str,
    token: str,
    notes_ref: str,
    registry: str,
    api_root: str,
    pr: dict[str, Any] | None = None,
    snapshot_source: str = "ci-main",
) -> dict[str, Any]:
    if pr is None:
        pr = load_pr_for_commit(api_root, repository, token, target_sha)
    if pr is None:
        raise SnapshotError(f"Commit {target_sha} is not associated with a merged pull request")

    labels = current_pr_labels(pr)
    type_label, channel_label = parse_release_labels(labels)
    release_bump = type_label.split(":", 1)[1]
    release_channel = channel_label.split(":", 1)[1]
    owner, _repo = repository.split("/", 1)
    image_name_lower = f"{owner.lower()}/dockrev"
    supervisor_image_name_lower = f"{owner.lower()}/dockrev-supervisor"

    snapshot: dict[str, Any] = {
        "schema_version": SNAPSHOT_SCHEMA_VERSION,
        "target_sha": target_sha,
        "pr_number": pr.get("number"),
        "pr_title": pr.get("title") or "",
        "registry": registry,
        "pr_head_sha": current_pr_head_sha(pr),
        "type_label": type_label,
        "channel_label": channel_label,
        "release_bump": release_bump,
        "release_channel": release_channel,
        "release_enabled": type_label not in {"type:docs", "type:skip"},
        "release_prerelease": False,
        "publish_latest": False,
        "image_name_lower": image_name_lower,
        "supervisor_image_name_lower": supervisor_image_name_lower,
        "base_stable_version": "",
        "next_stable_version": "",
        "app_effective_version": "",
        "release_tag": "",
        "tags_csv": "",
        "supervisor_tags_csv": "",
        "notes_ref": notes_ref,
        "snapshot_source": snapshot_source,
        "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }

    if snapshot["release_enabled"]:
        base = compute_base_stable_version(notes_ref, target_sha)
        next_stable = base.bump(release_bump)
        effective = next_stable.render()
        prerelease = False
        if release_channel == "rc":
            effective = f"{effective}-rc.{target_sha[:7]}"
            prerelease = True

        publish_latest = release_channel == "stable"
        release_tag = effective

        snapshot.update(
            {
                "base_stable_version": base.render(),
                "next_stable_version": next_stable.render(),
                "app_effective_version": effective,
                "release_tag": release_tag,
                "release_prerelease": prerelease,
                "publish_latest": publish_latest,
                "tags_csv": render_tags_csv(
                    image_name_lower,
                    registry,
                    release_tag,
                    publish_latest=publish_latest,
                ),
                "supervisor_tags_csv": render_tags_csv(
                    supervisor_image_name_lower,
                    registry,
                    release_tag,
                    publish_latest=publish_latest,
                ),
            }
        )

    return validate_snapshot(snapshot, expected_sha=target_sha)


def build_publication(
    snapshot: dict[str, Any],
    *,
    dockrev_digest: str,
    dockrev_supervisor_digest: str,
    published_at: str = "",
) -> dict[str, Any]:
    validated_snapshot = validate_snapshot(snapshot)
    if not validated_snapshot.get("release_enabled"):
        raise SnapshotError("Cannot record publication for a release-disabled snapshot")
    pr_number = validated_snapshot.get("pr_number")
    if not isinstance(pr_number, int) or pr_number <= 0:
        raise SnapshotError("Release snapshot pr_number must be present to record publication")

    publication = {
        "schema_version": PUBLICATION_SCHEMA_VERSION,
        "target_sha": validated_snapshot["target_sha"],
        "pr_number": pr_number,
        "release_tag": validated_snapshot["release_tag"],
        "release_channel": validated_snapshot["release_channel"],
        "published_at": published_at or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "dockrev_digest": validate_digest(dockrev_digest, field_name="dockrev_digest"),
        "dockrev_supervisor_digest": validate_digest(
            dockrev_supervisor_digest,
            field_name="dockrev_supervisor_digest",
        ),
    }
    return validate_publication(publication, expected_sha=str(validated_snapshot["target_sha"]))


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")


def export_key_values(values: dict[str, Any], github_output: str) -> None:
    lines = []
    for key, value in values.items():
        if isinstance(value, bool):
            rendered = "true" if value else "false"
        elif value is None:
            rendered = ""
        else:
            rendered = str(value)
        if "\n" in rendered:
            lines.append(f"{key}<<__CODEX__")
            lines.append(rendered)
            lines.append("__CODEX__")
        else:
            lines.append(f"{key}={rendered}")
    payload = "\n".join(lines) + "\n"
    if github_output:
        with Path(github_output).open("a", encoding="utf-8") as handle:
            handle.write(payload)
    else:
        sys.stdout.write(payload)


def export_snapshot(snapshot: dict[str, Any], github_output: str) -> None:
    export_key_values(
        {
            "target_sha": snapshot.get("target_sha"),
            "release_enabled": snapshot.get("release_enabled"),
            "release_bump": snapshot.get("release_bump"),
            "release_channel": snapshot.get("release_channel"),
            "pr_number": snapshot.get("pr_number"),
            "pr_title": snapshot.get("pr_title"),
            "image_name_lower": snapshot.get("image_name_lower"),
            "supervisor_image_name_lower": snapshot.get("supervisor_image_name_lower"),
            "app_effective_version": snapshot.get("app_effective_version"),
            "release_tag": snapshot.get("release_tag"),
            "release_prerelease": snapshot.get("release_prerelease"),
            "publish_latest": snapshot.get("publish_latest"),
            "tags_csv": snapshot.get("tags_csv"),
            "supervisor_tags_csv": snapshot.get("supervisor_tags_csv"),
            "snapshot_source": snapshot.get("snapshot_source"),
        },
        github_output,
    )


def ensure_snapshot(args: argparse.Namespace) -> int:
    target_sha = normalize_sha(args.target_sha)
    output_path = Path(args.output)
    snapshot_source = "manual-backfill" if args.target_only else "ci-main"

    for attempt in range(1, args.max_attempts + 1):
        fetch_notes_ref(args.notes_ref)
        existing = read_snapshot(args.notes_ref, target_sha)
        if existing is not None:
            write_json(output_path, existing)
            return 0

        commits_to_fill = commits_to_materialize(args.notes_ref, target_sha, target_only=args.target_only)
        target_snapshot: dict[str, Any] | None = None
        with tempfile.TemporaryDirectory(prefix="release-snapshot-notes-") as tmp:
            temp_note = Path(tmp) / "snapshot.json"
            for commit in commits_to_fill:
                snapshot = read_snapshot(args.notes_ref, commit)
                if snapshot is not None:
                    if commit == target_sha:
                        target_snapshot = snapshot
                        break
                    continue

                pr = load_pr_for_commit(
                    args.api_root,
                    args.github_repository,
                    args.github_token,
                    commit,
                    allow_zero=(commit != target_sha),
                )
                if pr is None:
                    continue

                snapshot = build_snapshot(
                    target_sha=commit,
                    repository=args.github_repository,
                    token=args.github_token,
                    notes_ref=args.notes_ref,
                    registry=args.registry,
                    api_root=args.api_root,
                    pr=pr,
                    snapshot_source=snapshot_source if commit == target_sha else "ci-main",
                )
                write_json(temp_note, snapshot)
                git("notes", f"--ref={args.notes_ref}", "add", "-f", "-F", str(temp_note), commit)
                if commit == target_sha:
                    target_snapshot = snapshot

        if target_snapshot is None:
            raise SnapshotError(f"Failed to materialize release snapshot for {target_sha}")

        write_json(output_path, target_snapshot)

        push = git("push", "origin", args.notes_ref, check=False)
        if push.returncode == 0:
            return 0

        if attempt == args.max_attempts:
            detail = push.stderr.strip() or push.stdout.strip() or "git push origin notes ref failed"
            raise SnapshotError(f"Failed to publish release snapshot after {attempt} attempts: {detail}")

    raise SnapshotError("release snapshot retry loop exhausted unexpectedly")


def export_existing_snapshot(args: argparse.Namespace) -> int:
    target_sha = normalize_sha(args.target_sha)
    fetch_notes_ref(args.notes_ref)
    snapshot = read_snapshot(args.notes_ref, target_sha)
    if snapshot is None:
        raise SnapshotError(f"Missing immutable release snapshot for {target_sha}")
    if args.resolve_publication_tags:
        if not args.main_ref:
            raise SnapshotError("--main-ref is required when --resolve-publication-tags is set")
        fetch_notes_ref(args.publication_notes_ref)
        snapshot = dict(snapshot)
        publish_latest = publish_latest_for_snapshot(
            snapshot,
            publication_notes_ref=args.publication_notes_ref,
            main_ref=args.main_ref,
        )
        snapshot["publish_latest"] = publish_latest
        snapshot["tags_csv"] = publication_tags(
            snapshot,
            publication_notes_ref=args.publication_notes_ref,
            main_ref=args.main_ref,
        )
        snapshot["supervisor_tags_csv"] = supervisor_publication_tags(
            snapshot,
            publication_notes_ref=args.publication_notes_ref,
            main_ref=args.main_ref,
        )
    export_snapshot(snapshot, args.github_output)
    return 0


def record_publication(args: argparse.Namespace) -> int:
    target_sha = normalize_sha(args.target_sha)
    output_path = Path(args.output) if args.output else None
    for attempt in range(1, args.max_attempts + 1):
        fetch_notes_ref(args.snapshot_notes_ref)
        fetch_notes_ref(args.publication_notes_ref)
        snapshot = read_snapshot(args.snapshot_notes_ref, target_sha)
        if snapshot is None:
            raise SnapshotError(f"Missing immutable release snapshot for {target_sha}")
        publication = build_publication(
            snapshot,
            dockrev_digest=args.dockrev_digest,
            dockrev_supervisor_digest=args.dockrev_supervisor_digest,
            published_at=args.published_at,
        )
        with tempfile.TemporaryDirectory(prefix="release-publication-notes-") as tmp:
            temp_note = Path(tmp) / "publication.json"
            write_json(temp_note, publication)
            git("notes", f"--ref={args.publication_notes_ref}", "add", "-f", "-F", str(temp_note), target_sha)
            if output_path is not None:
                write_json(output_path, publication)

        push = git("push", "origin", args.publication_notes_ref, check=False)
        if push.returncode == 0:
            return 0
        if attempt == args.max_attempts:
            detail = push.stderr.strip() or push.stdout.strip() or "git push origin publication notes ref failed"
            raise SnapshotError(f"Failed to publish release publication after {attempt} attempts: {detail}")
    raise SnapshotError("release publication retry loop exhausted unexpectedly")


def export_next_pending(args: argparse.Namespace) -> int:
    upper_bound = args.upper_bound or git_output("rev-parse", args.main_ref)
    upper_bound = normalize_sha(upper_bound)
    git("merge-base", "--is-ancestor", upper_bound, args.main_ref)
    fetch_notes_ref(args.notes_ref)
    fetch_tags()
    pending = pending_release_targets(args.notes_ref, upper_bound)
    export_key_values({"target_sha": pending[0] if pending else ""}, args.github_output)
    return 0


def main() -> int:
    args = parse_args()
    try:
        if args.command == "ensure":
            return ensure_snapshot(args)
        if args.command == "export":
            return export_existing_snapshot(args)
        if args.command == "next-pending":
            return export_next_pending(args)
        if args.command == "record-publication":
            return record_publication(args)
        raise SnapshotError(f"Unsupported command: {args.command}")
    except SnapshotError as exc:
        print(f"release_snapshot.py: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
