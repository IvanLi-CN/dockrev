#!/usr/bin/env python3
"""Resolve immutable publication-lock ownership to an exact merged identity."""

from __future__ import annotations

import base64
from collections.abc import Callable
from typing import Any

import release_policy


class PublicationLockOwnershipError(ValueError):
    pass


def _repository_parts(repository: str) -> tuple[str, str]:
    parts = repository.split("/", 1)
    if len(parts) != 2 or not all(parts):
        raise PublicationLockOwnershipError("repository must be owner/name")
    return parts[0], parts[1]


def _validate_identity_commit(
    fetch_json: Callable[[str], Any],
    repository: str,
    lock_sha: str,
    version: str,
) -> tuple[dict[str, str], str]:
    owner, name = _repository_parts(repository)
    commit = fetch_json(f"/repos/{owner}/{name}/commits/{lock_sha}")
    if not isinstance(commit, dict):
        raise PublicationLockOwnershipError("publication lock commit is invalid")
    commit_data = commit.get("commit")
    verification = commit_data.get("verification") if isinstance(commit_data, dict) else None
    if (
        not isinstance(commit_data, dict)
        or not isinstance(verification, dict)
        or verification.get("verified") is not True
    ):
        raise PublicationLockOwnershipError("publication lock identity is not signed")
    parents = commit.get("parents")
    if not isinstance(parents, list) or len(parents) != 1 or not isinstance(parents[0], dict):
        raise PublicationLockOwnershipError("publication lock identity must have one parent")
    parent_sha = parents[0].get("sha")
    try:
        release_policy.validate_sha(str(parent_sha), "publication lock identity parent SHA")
    except release_policy.PolicyError as error:
        raise PublicationLockOwnershipError(str(error)) from error
    files = commit.get("files")
    if (
        not isinstance(files, list)
        or not all(isinstance(item, dict) for item in files)
        or [item.get("filename") for item in files] != ["VERSION"]
    ):
        raise PublicationLockOwnershipError("publication lock identity must change VERSION only")

    try:
        trailers = release_policy.parse_trailers(str(commit_data.get("message", "")))
    except release_policy.PolicyError as error:
        raise PublicationLockOwnershipError(str(error)) from error
    if trailers.get("Product-Version") != version:
        raise PublicationLockOwnershipError("publication lock identity version does not match its ref")
    baseline = trailers.get("Release-Baseline-Version", "")
    try:
        release_policy.validate_final_baseline_version(baseline)
        intent = release_policy.parse_labels(trailers.get("Release-Intent", "").split())
        release_policy.validate_rfc3339_utc_timestamp(
            trailers.get("Source-PR-Updated-At", ""), "publication lock source_pr_updated_at"
        )
    except release_policy.PolicyError as error:
        raise PublicationLockOwnershipError(str(error)) from error
    if not intent["release_enabled"]:
        raise PublicationLockOwnershipError("publication lock identity must be release-enabled")

    content = fetch_json(f"/repos/{owner}/{name}/contents/VERSION?ref={lock_sha}")
    if not isinstance(content, dict) or content.get("encoding") != "base64":
        raise PublicationLockOwnershipError("publication lock VERSION content is invalid")
    try:
        encoded = "".join(str(content["content"]).split())
        identity_version = base64.b64decode(encoded, validate=True).decode().strip()
    except (KeyError, ValueError, UnicodeDecodeError) as error:
        raise PublicationLockOwnershipError("publication lock VERSION content is invalid") from error
    if identity_version != version:
        raise PublicationLockOwnershipError("publication lock VERSION does not match its provenance")
    try:
        release_policy.validate_channel_version(version, intent["channel"])
    except release_policy.PolicyError as error:
        raise PublicationLockOwnershipError(str(error)) from error

    mode = trailers.get("Release-Mode")
    if mode == "version-only-release-pr":
        covered_merge_sha = trailers.get("Covered-Product-Merge-SHA", "")
        if trailers.get("Source-SHA"):
            raise PublicationLockOwnershipError("version-only publication lock cannot carry Source-SHA")
        try:
            release_policy.validate_sha(covered_merge_sha, "publication identity covered merge SHA")
            covered_version = _version_at_commit(fetch_json, repository, covered_merge_sha)
            release_policy.validate_preparation_version(
                covered_version, version, intent, baseline_version=baseline
            )
        except release_policy.PolicyError as error:
            raise PublicationLockOwnershipError(str(error)) from error
        return trailers, covered_merge_sha

    if mode == "normal-preparation":
        source_sha = trailers.get("Source-SHA", "")
        if trailers.get("Covered-Product-Merge-SHA"):
            raise PublicationLockOwnershipError("normal publication lock cannot carry covered merge provenance")
        try:
            release_policy.validate_sha(source_sha, "publication identity source SHA")
            source_version = _version_at_commit(fetch_json, repository, source_sha)
            release_policy.validate_preparation(
                {
                    "commit_sha": lock_sha,
                    "source_sha": source_sha,
                    "source_pr_updated_at": trailers.get("Source-PR-Updated-At", ""),
                    "version": version,
                    "baseline_version": baseline,
                    "intent": intent,
                    "release_mode": mode,
                    "parents": [parent_sha],
                    "changed_files": ["VERSION"],
                    "verified": True,
                },
                source_sha=source_sha,
            )
            release_policy.validate_preparation_version(
                source_version, version, intent, baseline_version=baseline
            )
        except release_policy.PolicyError as error:
            raise PublicationLockOwnershipError(str(error)) from error
        return trailers, source_sha

    raise PublicationLockOwnershipError("publication lock identity has an unsupported release mode")


def _version_at_commit(fetch_json: Callable[[str], Any], repository: str, commit_sha: str) -> str:
    owner, name = _repository_parts(repository)
    content = fetch_json(f"/repos/{owner}/{name}/contents/VERSION?ref={commit_sha}")
    if not isinstance(content, dict) or content.get("encoding") != "base64":
        raise PublicationLockOwnershipError("publication lock source VERSION content is invalid")
    try:
        encoded = "".join(str(content["content"]).split())
        version = base64.b64decode(encoded, validate=True).decode().strip()
        release_policy.parse_version(version)
    except (KeyError, ValueError, UnicodeDecodeError, release_policy.PolicyError) as error:
        raise PublicationLockOwnershipError("publication lock source VERSION content is invalid") from error
    return version


def _merged_main_associations(
    fetch_json: Callable[[str], Any], repository: str, source_sha: str
) -> list[dict[str, Any]]:
    owner, name = _repository_parts(repository)
    associations: list[dict[str, Any]] = []
    page = 1
    while True:
        pulls = fetch_json(
            f"/repos/{owner}/{name}/commits/{source_sha}/pulls?per_page=100&page={page}"
        )
        if not isinstance(pulls, list):
            raise PublicationLockOwnershipError("publication lock pull request associations are invalid")
        for pull in pulls:
            if not isinstance(pull, dict):
                raise PublicationLockOwnershipError("publication lock pull request association is invalid")
            base = pull.get("base")
            head = pull.get("head")
            if not isinstance(base, dict) or not isinstance(head, dict):
                raise PublicationLockOwnershipError("publication lock pull request association is incomplete")
            base_ref = base.get("ref")
            if not isinstance(base_ref, str) or not base_ref:
                raise PublicationLockOwnershipError("publication lock pull request base ref is invalid")
            head_sha = head.get("sha")
            state = pull.get("state")
            if not isinstance(state, str) or state not in {"open", "closed"}:
                raise PublicationLockOwnershipError("publication lock pull request state is invalid")
            merged_at = pull.get("merged_at")
            if merged_at is not None and not isinstance(merged_at, str):
                raise PublicationLockOwnershipError("publication lock pull request merge time is invalid")
            try:
                release_policy.validate_sha(str(head_sha), "publication identity pull request head SHA")
            except release_policy.PolicyError as error:
                raise PublicationLockOwnershipError(str(error)) from error
            if base_ref != "main":
                continue
            if state != "closed" or not merged_at:
                continue
            merge_sha = pull.get("merge_commit_sha")
            try:
                release_policy.validate_sha(str(merge_sha), "publication identity merge SHA")
            except release_policy.PolicyError as error:
                raise PublicationLockOwnershipError(str(error)) from error
            associations.append(pull)
        if len(pulls) < 100:
            return associations
        page += 1


def publication_lock_matches_merge(
    fetch_json: Callable[[str], Any],
    repository: str,
    lock_sha: str,
    version: str,
    covered_merge_sha: str,
    *,
    allowed_identity_shas: set[str] | None = None,
) -> bool:
    """Return whether a lock is bound to this merge; fail closed if unresolved."""
    try:
        release_policy.validate_sha(lock_sha, "publication lock commit SHA")
        release_policy.validate_sha(covered_merge_sha, "covered product merge SHA")
        for identity_sha in allowed_identity_shas or set():
            release_policy.validate_sha(identity_sha, "allowed publication identity SHA")
    except release_policy.PolicyError as error:
        raise PublicationLockOwnershipError(str(error)) from error

    if lock_sha in (allowed_identity_shas or set()):
        return False

    trailers, identity_source_sha = _validate_identity_commit(
        fetch_json, repository, lock_sha, version
    )
    if trailers["Release-Mode"] == "version-only-release-pr":
        return trailers["Covered-Product-Merge-SHA"] == covered_merge_sha

    associations = _merged_main_associations(fetch_json, repository, identity_source_sha)
    if len(associations) != 1:
        raise PublicationLockOwnershipError(
            "publication lock identity does not resolve to exactly one merged main PR"
        )
    return associations[0].get("merge_commit_sha") == covered_merge_sha
