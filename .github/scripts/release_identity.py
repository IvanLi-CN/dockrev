#!/usr/bin/env python3
"""Resolve a merged main commit into one immutable release identity."""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.parse
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

import release_policy


class IdentityError(RuntimeError):
    pass


def api_json(api_root: str, token: str, path: str) -> Any:
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}{path}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "dockrev-release-identity",
        },
    )
    try:
        with urllib.request.urlopen(request) as response:
            return json.loads(response.read().decode(response.headers.get_content_charset() or "utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        raise IdentityError(f"GitHub API failed: {error.code}: {detail[:400]}") from error


def repository_parts(repository: str) -> tuple[str, str]:
    owner, separator, name = repository.partition("/")
    if not separator or not owner or not name:
        raise IdentityError("repository must be owner/name")
    return owner, name


def merged_pr(api_root: str, token: str, repository: str, merge_sha: str) -> tuple[dict[str, Any], dict[str, Any]]:
    owner, name = repository_parts(repository)
    pulls = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{merge_sha}/pulls")
    if not isinstance(pulls, list) or len(pulls) != 1:
        raise IdentityError("merged release commit must map to exactly one product PR")
    pr = pulls[0]
    if pr.get("base", {}).get("ref") != "main" or pr.get("state") != "closed" or not pr.get("merged_at"):
        raise IdentityError("release commit is not one merged main product PR")
    if pr.get("merge_commit_sha") != merge_sha:
        raise IdentityError("release commit is not the exact PR merge SHA")
    return pr, {}


def intent_from_trailer(value: str) -> dict[str, Any]:
    labels = value.split()
    if len(labels) != 2:
        raise IdentityError("release intent trailer must contain type and channel labels")
    try:
        return release_policy.parse_labels(labels)
    except release_policy.PolicyError as error:
        raise IdentityError(str(error)) from error


def version_at_commit(api_root: str, token: str, repository: str, commit_sha: str) -> str:
    owner, name = repository_parts(repository)
    payload = api_json(
        api_root,
        token,
        f"/repos/{owner}/{name}/contents/VERSION?ref={urllib.parse.quote(commit_sha, safe='')}",
    )
    if payload.get("encoding") != "base64":
        raise IdentityError("merged identity VERSION is not returned as base64")
    try:
        import base64

        encoded_content = "".join(str(payload["content"]).split())
        version = base64.b64decode(encoded_content, validate=True).decode().strip()
    except (KeyError, ValueError, UnicodeDecodeError) as error:
        raise IdentityError("merged identity VERSION is not valid UTF-8 base64") from error
    release_policy.parse_version(version)
    return version


def version_exists_at_commit(api_root: str, token: str, repository: str, commit_sha: str) -> bool:
    try:
        version_at_commit(api_root, token, repository, commit_sha)
    except IdentityError as error:
        if "GitHub API failed: 404" in str(error):
            return False
        raise
    return True


def covered_product_boundary(
    api_root: str, token: str, repository: str, covered_merge_sha: str
) -> tuple[dict[str, Any], str]:
    pr, _ = merged_pr(api_root, token, repository, covered_merge_sha)
    owner, name = repository_parts(repository)
    head_sha = pr.get("head", {}).get("sha", "")
    release_policy.validate_sha(head_sha, "covered_product_head_sha")
    try:
        covered_intent = release_policy.parse_labels(
            [item.get("name") for item in pr.get("labels", []) if item.get("name")]
        )
    except release_policy.PolicyError as error:
        raise IdentityError(f"covered product labels are invalid: {error}") from error
    if not covered_intent["release_enabled"]:
        raise IdentityError("version-only release PR must cover a release-enabled product PR")
    commit = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{head_sha}")
    trailers = release_policy.parse_trailers(commit.get("commit", {}).get("message", ""))
    if any(
        trailers.get(key)
        for key in ("Release-Mode", "Source-SHA", "Product-Version", "Release-Intent", "Covered-Product-Merge-SHA")
    ):
        raise IdentityError("covered product PR already has release identity")
    return pr, head_sha


def pull_request_changed_files(
    api_root: str, token: str, repository: str, pr_number: int
) -> list[str]:
    owner, name = repository_parts(repository)
    files: list[str] = []
    page = 1
    while True:
        batch = api_json(
            api_root,
            token,
            f"/repos/{owner}/{name}/pulls/{pr_number}/files?per_page=100&page={page}",
        )
        if not isinstance(batch, list):
            raise IdentityError("GitHub PR file list is invalid")
        files.extend(str(item.get("filename")) for item in batch if item.get("filename"))
        if len(batch) < 100:
            return sorted(set(files))
        page += 1


def resolve_from_payload(payload: dict[str, Any]) -> dict[str, Any]:
    intent = payload.get("intent") or release_policy.parse_labels(payload.get("labels", []))
    if payload.get("type") == "none" or intent.get("type") == "none":
        identity_fields = (
            "release_mode", "version", "version_file", "source_sha", "merge_commit_sha",
            "preparation_commit_sha", "covered_product_merge_sha", "provenance", "artifact_names",
        )
        if any(payload.get(field) not in (None, "", [], {}) for field in identity_fields):
            raise IdentityError("type:none payload cannot carry release identity")
        return {"release_enabled": False, "reason": "type:none"}
    version = str(payload.get("version", ""))
    version_file = str(payload.get("version_file", ""))
    if not version_file or version != version_file:
        raise IdentityError("release identity VERSION disagrees with provenance")
    release_policy.validate_channel_version(version, intent["channel"])
    identity = {
        "schema_version": 1,
        "release_enabled": True,
        "pull_request": payload.get("pull_request"),
        "source_sha": payload.get("source_sha"),
        "merge_commit_sha": payload.get("merge_commit_sha"),
        "preparation_commit_sha": payload.get("preparation_commit_sha"),
        "covered_product_merge_sha": payload.get("covered_product_merge_sha"),
        "release_mode": payload.get("release_mode"),
        "version": version,
        "release_tag": f"v{version}",
        "intent": intent,
        "type": intent["type"],
        "channel": intent["channel"],
        "components": intent.get("components", []),
        "artifact_names": payload.get("artifact_names", []),
        "run_url": payload.get("run_url", ""),
    }
    release_policy.validate_identity(identity)
    return identity


def resolve_github(api_root: str, token: str, repository: str, merge_sha: str, run_url: str = "") -> dict[str, Any]:
    release_policy.validate_sha(merge_sha, "merge_commit_sha")
    pr, _ = merged_pr(api_root, token, repository, merge_sha)
    labels = [item.get("name") for item in pr.get("labels", []) if item.get("name")]
    try:
        pr_intent = release_policy.parse_labels(labels)
    except release_policy.PolicyError as error:
        raise IdentityError(f"merged product PR labels are invalid: {error}") from error
    owner, name = repository_parts(repository)
    head_sha = pr.get("head", {}).get("sha", "")
    head_commit = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{head_sha}")
    trailers = release_policy.parse_trailers(head_commit.get("commit", {}).get("message", ""))
    if not pr_intent["release_enabled"]:
        if any(
            trailers.get(key)
            for key in ("Release-Mode", "Source-SHA", "Product-Version", "Release-Intent", "Covered-Product-Merge-SHA")
        ):
            raise IdentityError("type:none labels conflict with merged release identity")
        changed_files = pull_request_changed_files(api_root, token, repository, pr.get("number", 0))
        if "VERSION" in changed_files:
            base_sha = pr.get("base", {}).get("sha", "")
            if not base_sha:
                merge_commit = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{merge_sha}")
                parents = [parent.get("sha") for parent in merge_commit.get("parents", [])]
                base_sha = parents[0] if parents else ""
            if not base_sha or version_exists_at_commit(api_root, token, repository, base_sha):
                raise IdentityError("type:none merged PR cannot change an existing VERSION")
            version_at_commit(api_root, token, repository, merge_sha)
        return {
            "release_enabled": False,
            "merge_commit_sha": merge_sha,
            "pull_request": pr.get("number"),
            "reason": "version-bootstrap" if "VERSION" in changed_files else "type:none",
        }
    mode = trailers.get("Release-Mode")
    if mode not in {"normal-preparation", "version-only-release-pr"}:
        if mode or any(trailers.get(key) for key in ("Source-SHA", "Product-Version", "Release-Intent", "Covered-Product-Merge-SHA")):
            raise IdentityError("merged product PR has malformed release identity")
        return {
            "release_enabled": False,
            "merge_commit_sha": merge_sha,
            "pull_request": pr.get("number"),
            "reason": "no-release-identity",
        }
    version = trailers.get("Product-Version", "")
    if not version:
        raise IdentityError("merged product PR is missing Product-Version provenance")
    changed_files = sorted({entry.get("filename") for entry in head_commit.get("files", []) if entry.get("filename")})
    parents = [parent.get("sha") for parent in head_commit.get("parents", [])]
    verified = head_commit.get("commit", {}).get("verification", {}).get("verified") is True
    release_intent = trailers.get("Release-Intent", "")
    intent = intent_from_trailer(release_intent)
    if (
        intent["type_label"] != pr_intent["type_label"]
        or intent["channel_label"] != pr_intent["channel_label"]
    ):
        raise IdentityError("merged PR labels do not match frozen release intent")
    version_file = version_at_commit(api_root, token, repository, merge_sha)
    if version_file != version:
        raise IdentityError("merged commit VERSION does not match Product-Version provenance")
    if mode == "normal-preparation":
        if trailers.get("Covered-Product-Merge-SHA"):
            raise IdentityError("normal preparation identity cannot carry Covered-Product-Merge-SHA")
        source_sha = trailers.get("Source-SHA", "")
        source_version = version_at_commit(api_root, token, repository, source_sha)
        preparation = {
            "commit_sha": head_sha,
            "source_sha": source_sha,
            "version": version,
            "intent": intent,
            "source_version": source_version,
            "release_intent": release_intent,
            "release_mode": mode,
            "parents": parents,
            "changed_files": changed_files,
            "verified": verified,
        }
        try:
            release_policy.validate_preparation(preparation)
            release_policy.validate_preparation_version(source_version, version, intent)
        except release_policy.PolicyError as error:
            raise IdentityError(str(error)) from error
    else:
        if trailers.get("Source-SHA"):
            raise IdentityError("version-only release identity cannot carry Source-SHA")
        covered_merge_sha = trailers.get("Covered-Product-Merge-SHA", "")
        covered_pr, covered_head_sha = covered_product_boundary(api_root, token, repository, covered_merge_sha)
        changed_files = pull_request_changed_files(api_root, token, repository, pr.get("number", 0))
        provenance = {
            "covered_product_merge_sha": covered_merge_sha,
            "covered_product_pr_number": covered_pr.get("number", 0),
            "covered_product_head_sha": covered_head_sha,
            "covered_product_merged": True,
            "covered_product_has_identity": False,
            "product_version": version,
            "release_intent": release_intent,
            "release_mode": mode,
            "branch_head_sha": head_sha,
            "verified": verified,
        }
        try:
            release_policy.validate_version_only(changed_files, provenance, head_sha=head_sha)
        except release_policy.PolicyError as error:
            raise IdentityError(str(error)) from error
    payload = {
        "pull_request": pr.get("number"),
        "source_sha": covered_head_sha if mode == "version-only-release-pr" else trailers.get("Source-SHA", head_sha),
        "merge_commit_sha": merge_sha,
        "preparation_commit_sha": head_sha if mode == "normal-preparation" else None,
        "covered_product_merge_sha": trailers.get("Covered-Product-Merge-SHA"),
        "release_mode": mode,
        "version": version,
        "version_file": version_file,
        "type": intent["type"],
        "intent": intent,
        "artifact_names": [],
        "run_url": run_url,
    }
    if mode == "version-only-release-pr" and not payload["covered_product_merge_sha"]:
        raise IdentityError("version-only release PR is missing Covered-Product-Merge-SHA")
    return resolve_from_payload(payload)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path)
    parser.add_argument("--repository")
    parser.add_argument("--merge-sha")
    parser.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))
    parser.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    parser.add_argument("--run-url", default="")
    args = parser.parse_args()
    try:
        if args.input:
            result = resolve_from_payload(json.loads(args.input.read_text(encoding="utf-8")))
        else:
            if not args.repository or not args.merge_sha or not args.token:
                raise IdentityError("repository, merge-sha and token are required without --input")
            result = resolve_github(args.api_root, args.token, args.repository, args.merge_sha, args.run_url)
        print(json.dumps(result, sort_keys=True, indent=2))
        return 3 if result.get("release_enabled") is False and result.get("reason") == "no-release-identity" else 0
    except (IdentityError, release_policy.PolicyError, OSError, ValueError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
