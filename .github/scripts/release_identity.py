#!/usr/bin/env python3
"""Resolve a merged main commit into one immutable release identity."""

from __future__ import annotations

import argparse
import json
import os
import sys
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
    labels = [item["name"] for item in pr.get("labels", []) if item.get("name")]
    intent = release_policy.parse_labels(labels)
    return pr, intent


def resolve_from_payload(payload: dict[str, Any]) -> dict[str, Any]:
    if payload.get("type") == "none":
        return {"release_enabled": False, "reason": "type:none"}
    intent = payload.get("intent") or release_policy.parse_labels(payload.get("labels", []))
    version = str(payload.get("version", ""))
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
    pr, intent = merged_pr(api_root, token, repository, merge_sha)
    if not intent["release_enabled"]:
        return {"release_enabled": False, "merge_commit_sha": merge_sha, "pull_request": pr.get("number"), "reason": "type:none"}
    owner, name = repository_parts(repository)
    head_sha = pr.get("head", {}).get("sha", "")
    head_commit = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{head_sha}")
    trailers = release_policy.parse_trailers(head_commit.get("commit", {}).get("message", ""))
    mode = trailers.get("Release-Mode")
    if mode not in {"normal-preparation", "version-only-release-pr"}:
        raise IdentityError("merged product PR has no accepted release provenance")
    version = trailers.get("Product-Version", "")
    if not version:
        raise IdentityError("merged product PR is missing Product-Version provenance")
    changed_files = sorted({entry.get("filename") for entry in head_commit.get("files", []) if entry.get("filename")})
    parents = [parent.get("sha") for parent in head_commit.get("parents", [])]
    verified = head_commit.get("commit", {}).get("verification", {}).get("verified") is True
    release_intent = f"{intent['type_label']} {intent['channel_label']}"
    if trailers.get("Release-Intent") != release_intent:
        raise IdentityError("merged product PR release intent does not match its labels")
    if mode == "normal-preparation":
        preparation = {
            "commit_sha": head_sha,
            "source_sha": trailers.get("Source-SHA", ""),
            "version": version,
            "intent": intent,
            "release_mode": mode,
            "parents": parents,
            "changed_files": changed_files,
            "verified": verified,
        }
        try:
            release_policy.validate_preparation(preparation)
        except release_policy.PolicyError as error:
            raise IdentityError(str(error)) from error
    else:
        provenance = {
            "covered_product_merge_sha": trailers.get("Covered-Product-Merge-SHA", ""),
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
        "source_sha": trailers.get("Source-SHA", head_sha),
        "merge_commit_sha": merge_sha,
        "preparation_commit_sha": head_sha if mode == "normal-preparation" else None,
        "covered_product_merge_sha": trailers.get("Covered-Product-Merge-SHA"),
        "release_mode": mode,
        "version": version,
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
        return 0
    except (IdentityError, release_policy.PolicyError, OSError, ValueError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
