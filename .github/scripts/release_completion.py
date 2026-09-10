#!/usr/bin/env python3
"""Validate the required Release completion check for one PR head."""

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


class CompletionError(RuntimeError):
    pass


def api_json(api_root: str, token: str, path: str) -> Any:
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}{path}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "dockrev-release-completion",
        },
    )
    try:
        with urllib.request.urlopen(request) as response:
            return json.loads(response.read().decode(response.headers.get_content_charset() or "utf-8"))
    except urllib.error.HTTPError as error:
        raise CompletionError(f"GitHub API failed: {error.code}") from error


def validate_source_checks(payload: dict[str, Any]) -> None:
    required = {"ci_pr", "label_gate"}
    if required - set(payload):
        raise CompletionError("source check evidence is incomplete")
    for name in required:
        value = payload[name]
        if not isinstance(value, dict) or value.get("status") != "completed" or value.get("conclusion") != "success":
            raise CompletionError(f"source check did not pass: {name}")


def validate_completion(payload: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(payload, dict):
        raise CompletionError("completion input must be an object")
    intent = release_policy.parse_labels(payload.get("labels", []))
    if not intent["release_enabled"]:
        return {"status": "pass", "release_enabled": False, "mode": "non-product"}
    source_sha = payload.get("source_sha")
    head_sha = payload.get("head_sha")
    release_policy.validate_sha(str(source_sha), "source_sha")
    release_policy.validate_sha(str(head_sha), "head_sha")
    validate_source_checks(payload.get("source_checks", {}))
    mode = payload.get("release_mode")
    if mode == "normal-preparation":
        preparation = payload.get("preparation")
        if not isinstance(preparation, dict):
            raise CompletionError("normal preparation evidence is missing")
        release_policy.validate_preparation(preparation, source_sha=source_sha)
        if preparation.get("intent", {}).get("labels") != intent.get("labels"):
            raise CompletionError("preparation release intent does not match current PR labels")
        if preparation.get("commit_sha") != head_sha:
            raise CompletionError("PR head is not the verified preparation commit")
        if payload.get("base_ref") != "main":
            raise CompletionError("release PR must target main")
        release_policy.validate_channel_version(str(preparation["version"]), intent["channel"])
        if payload.get("tag_reserved") is not True:
            raise CompletionError("derived release tag is not reserved")
    elif mode == "version-only-release-pr":
        release_policy.validate_version_only(payload.get("changed_files", []), payload.get("provenance", {}), head_sha=head_sha)
        if payload["provenance"].get("release_intent") != f"{intent['type_label']} {intent['channel_label']}":
            raise CompletionError("version-only release intent does not match current PR labels")
        release_policy.validate_channel_version(str(payload["provenance"]["product_version"]), intent["channel"])
        if payload.get("tag_reserved") is not True:
            raise CompletionError("version-only release PR tag is not reserved")
    else:
        raise CompletionError("PR has no allowed release identity mode")
    return {
        "status": "pass",
        "release_enabled": True,
        "mode": mode,
        "source_sha": source_sha,
        "head_sha": head_sha,
        "version": payload.get("version") or payload.get("preparation", {}).get("version") or payload.get("provenance", {}).get("product_version"),
        "intent": intent,
    }


def tag_is_available(api_root: str, token: str, repository: str, version: str) -> bool:
    owner, name = repository.split("/", 1)
    try:
        api_json(api_root, token, f"/repos/{owner}/{name}/git/ref/tags/v{version}")
    except CompletionError as error:
        if "GitHub API failed: 404" in str(error):
            return True
        raise
    return False


def load_github_completion(api_root: str, token: str, repository: str, pr_number: int) -> dict[str, Any]:
    owner, name = repository.split("/", 1)
    pr = api_json(api_root, token, f"/repos/{owner}/{name}/pulls/{pr_number}")
    if pr.get("base", {}).get("ref") != "main" or pr.get("state") != "open":
        raise CompletionError("Release completion requires an open PR targeting main")
    labels = [item["name"] for item in pr.get("labels", []) if item.get("name")]
    head_sha = pr.get("head", {}).get("sha", "")
    source_sha = head_sha
    preparation = None
    provenance = None
    commit = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{head_sha}")
    parents = [parent.get("sha") for parent in commit.get("parents", [])]
    files = sorted({entry.get("filename") for entry in commit.get("files", []) if entry.get("filename")})
    trailers = release_policy.parse_trailers(commit.get("commit", {}).get("message", ""))
    mode = trailers.get("Release-Mode")
    if mode == "normal-preparation":
        source_sha = trailers.get("Source-SHA", "")
        preparation = {
            "commit_sha": head_sha,
            "source_sha": source_sha,
            "version": trailers.get("Product-Version", ""),
            "intent": release_policy.parse_labels(labels),
            "release_mode": mode,
            "parents": parents,
            "changed_files": files,
            "verified": commit.get("commit", {}).get("verification", {}).get("verified") is True,
        }
    elif mode == "version-only-release-pr":
        source_sha = trailers.get("Covered-Product-Merge-SHA", head_sha)
        provenance = {
            "covered_product_merge_sha": trailers.get("Covered-Product-Merge-SHA", ""),
            "product_version": trailers.get("Product-Version", ""),
            "release_intent": trailers.get("Release-Intent", ""),
            "release_mode": mode,
            "branch_head_sha": head_sha,
            "verified": commit.get("commit", {}).get("verification", {}).get("verified") is True,
        }
    else:
        provenance = None
    check_sha = source_sha
    if mode == "version-only-release-pr" and source_sha != head_sha:
        covered_pulls = api_json(api_root, token, f"/repos/{owner}/{name}/commits/{source_sha}/pulls")
        if isinstance(covered_pulls, list) and len(covered_pulls) == 1:
            check_sha = covered_pulls[0].get("head", {}).get("sha", source_sha)
    ci_runs = api_json(api_root, token, f"/repos/{owner}/{name}/actions/workflows/ci-pr.yml/runs?head_sha={check_sha}&per_page=100").get("workflow_runs", [])
    label_runs = api_json(api_root, token, f"/repos/{owner}/{name}/actions/workflows/label-gate.yml/runs?head_sha={check_sha}&per_page=100").get("workflow_runs", [])
    ci_run = next((run for run in ci_runs if run.get("head_sha") == check_sha), None)
    label_gate = next((run for run in label_runs if run.get("head_sha") == check_sha), None)
    if mode == "normal-preparation":
        version_for_tag = preparation["version"]
    elif mode == "version-only-release-pr":
        version_for_tag = provenance["product_version"]
    else:
        raise CompletionError("PR head has no accepted release provenance")
    payload = {
        "labels": labels,
        "head_sha": head_sha,
        "source_sha": source_sha,
        "base_ref": pr.get("base", {}).get("ref"),
        "release_mode": mode,
        "preparation": preparation,
        "changed_files": files,
        "provenance": provenance,
        "source_checks": {"ci_pr": ci_run or {}, "label_gate": label_gate or {}},
        "tag_reserved": tag_is_available(
            api_root,
            token,
            repository,
            version_for_tag,
        ),
    }
    return payload


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path)
    parser.add_argument("--repository")
    parser.add_argument("--pr-number", type=int)
    parser.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))
    parser.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    args = parser.parse_args()
    try:
        if args.input:
            payload = json.loads(args.input.read_text(encoding="utf-8"))
        else:
            if not args.repository or not args.pr_number or not args.token:
                raise CompletionError("repository, pr-number and token are required without --input")
            payload = load_github_completion(args.api_root, args.token, args.repository, args.pr_number)
        print(json.dumps(validate_completion(payload), sort_keys=True))
        return 0
    except (CompletionError, release_policy.PolicyError, OSError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
