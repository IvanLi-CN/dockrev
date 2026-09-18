#!/usr/bin/env python3
"""Resolve immutable publication-lock ownership to an exact merged identity."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

import release_policy


class PublicationLockOwnershipError(ValueError):
    pass


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
    if lock_sha == covered_merge_sha:
        return True

    try:
        owner, name = repository.split("/", 1)
    except ValueError as error:
        raise PublicationLockOwnershipError("repository must be owner/name") from error
    commit = fetch_json(f"/repos/{owner}/{name}/commits/{lock_sha}")
    if not isinstance(commit, dict):
        raise PublicationLockOwnershipError("publication lock commit is invalid")
    commit_data = commit.get("commit", {})
    verification = commit_data.get("verification") if isinstance(commit_data, dict) else None
    if not isinstance(commit_data, dict) or not isinstance(verification, dict) or verification.get("verified") is not True:
        raise PublicationLockOwnershipError("publication lock identity is not signed")

    trailers = release_policy.parse_trailers(str(commit_data.get("message", "")))
    if trailers.get("Product-Version") != version:
        raise PublicationLockOwnershipError("publication lock identity version does not match its ref")

    mode = trailers.get("Release-Mode")
    if mode == "version-only-release-pr":
        identity_merge_sha = trailers.get("Covered-Product-Merge-SHA", "")
        try:
            release_policy.validate_sha(identity_merge_sha, "publication identity covered merge SHA")
        except release_policy.PolicyError as error:
            raise PublicationLockOwnershipError(str(error)) from error
        return identity_merge_sha == covered_merge_sha

    if mode == "normal-preparation":
        source_sha = trailers.get("Source-SHA", "")
        try:
            release_policy.validate_sha(source_sha, "publication identity source SHA")
        except release_policy.PolicyError as error:
            raise PublicationLockOwnershipError(str(error)) from error
        pulls = fetch_json(f"/repos/{owner}/{name}/commits/{source_sha}/pulls")
        if not isinstance(pulls, list):
            raise PublicationLockOwnershipError("publication lock pull request associations are invalid")
        merged_main_shas: set[str] = set()
        for pull in pulls:
            base = pull.get("base") if isinstance(pull, dict) else None
            if (
                not isinstance(pull, dict)
                or not isinstance(base, dict)
                or base.get("ref") != "main"
                or pull.get("state") != "closed"
                or not pull.get("merged_at")
            ):
                continue
            merge_sha = pull.get("merge_commit_sha")
            try:
                release_policy.validate_sha(str(merge_sha), "publication identity merge SHA")
            except release_policy.PolicyError as error:
                raise PublicationLockOwnershipError(str(error)) from error
            merged_main_shas.add(str(merge_sha))
        if len(merged_main_shas) != 1:
            raise PublicationLockOwnershipError(
                "publication lock identity does not resolve to exactly one merged main PR"
            )
        return covered_merge_sha in merged_main_shas

    raise PublicationLockOwnershipError("publication lock identity has an unsupported release mode")
