#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

API_VERSION = "2022-11-28"
REPOSITORY_ROLE_IDS = {
    "maintain": 2,
    "admin": 5,
}
BYPASS_MODE_MAP = {
    0: "always",
    1: "pull_request",
    2: "exempt",
}


class ValidationError(RuntimeError):
    pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate the live GitHub rules on a branch against .github/quality-gates.json."
    )
    parser.add_argument(
        "--declaration",
        default=".github/quality-gates.json",
        help="Path to the quality gates declaration file.",
    )
    parser.add_argument(
        "--repo",
        default=os.environ.get("GITHUB_REPOSITORY", ""),
        help="GitHub repository in owner/name form. Defaults to GITHUB_REPOSITORY.",
    )
    parser.add_argument(
        "--branch",
        default="",
        help="Protected branch to validate. Defaults to the first protected branch in the declaration.",
    )
    parser.add_argument(
        "--api-root",
        default=os.environ.get("GITHUB_API_URL", "https://api.github.com"),
        help="GitHub API root URL. Defaults to GITHUB_API_URL or https://api.github.com.",
    )
    parser.add_argument(
        "--mode",
        choices=("auto", "require", "skip"),
        default=os.environ.get("QUALITY_GATES_LIVE_RULES_MODE", "auto"),
        help="skip: never validate; auto: validate only on GitHub Actions; require: always validate.",
    )
    return parser.parse_args()


def should_skip(mode: str) -> bool:
    if mode == "skip":
        print("[live-quality-gates] skipped: QUALITY_GATES_LIVE_RULES_MODE=skip")
        return True
    if mode == "auto" and os.environ.get("GITHUB_ACTIONS") != "true":
        print("[live-quality-gates] skipped: outside GitHub Actions")
        return True
    return False


def load_declaration(path: str) -> dict:
    data = json.loads(Path(path).read_text())
    if not isinstance(data, dict):
        raise ValidationError("quality-gates declaration must be a JSON object")
    return data


def choose_branch(declaration: dict, override: str) -> str:
    if override:
        return override
    branches = (
        declaration.get("policy", {})
        .get("branch_protection", {})
        .get("protected_branches", [])
    )
    if not isinstance(branches, list) or not branches:
        raise ValidationError("protected_branches must declare at least one protected branch")
    branch = branches[0]
    if not isinstance(branch, str) or not branch:
        raise ValidationError("protected_branches[0] must be a non-empty string")
    return branch


def split_repo(repo: str) -> tuple[str, str]:
    parts = repo.split("/", 1)
    if len(parts) != 2 or not parts[0] or not parts[1]:
        raise ValidationError("--repo must be in owner/name form")
    return parts[0], parts[1]


def fetch_json(api_root: str, owner: str, repo: str, branch: str) -> object:
    path = "/repos/{owner}/{repo}/rules/branches/{branch}?per_page=100".format(
        owner=urllib.parse.quote(owner, safe=""),
        repo=urllib.parse.quote(repo, safe=""),
        branch=urllib.parse.quote(branch, safe=""),
    )
    url = api_root.rstrip("/") + path
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "quality-gates-live-check/1.0",
        "X-GitHub-Api-Version": API_VERSION,
    }
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN") or ""
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        raise ValidationError(f"GitHub API request failed ({exc.code}): {detail}") from exc
    except urllib.error.URLError as exc:
        raise ValidationError(f"GitHub API request failed: {exc.reason}") from exc


def normalize_bypass_mode(value: object) -> str:
    if isinstance(value, str):
        return value
    if isinstance(value, int) and value in BYPASS_MODE_MAP:
        return BYPASS_MODE_MAP[value]
    return ""


def normalize_actor_id(value: object) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, str) and value.isdigit():
        return int(value)
    return None


def extract_rulesets(payload: object) -> list[dict]:
    if isinstance(payload, dict) and isinstance(payload.get("data"), list):
        payload = payload["data"]

    if isinstance(payload, list):
        items = [item for item in payload if isinstance(item, dict)]
    elif isinstance(payload, dict):
        items = [payload]
    else:
        raise ValidationError("Unsupported GitHub rules payload type")

    if not items:
        raise ValidationError("GitHub rules payload was empty")

    rulesets: list[dict] = []
    for item in items:
        rules = item.get("rules")
        if isinstance(rules, list):
            rulesets.append(item)
            continue
        if isinstance(item.get("type"), str):
            rulesets.append({"name": "<flattened-rules>", "rules": [item], "bypass_actors": []})
            continue
        raise ValidationError("Unsupported GitHub rules payload: missing rules array")
    return rulesets


def flatten_rules(rulesets: list[dict]) -> list[dict]:
    result: list[dict] = []
    for ruleset in rulesets:
        rules = ruleset.get("rules")
        if not isinstance(rules, list):
            continue
        for rule in rules:
            if isinstance(rule, dict) and isinstance(rule.get("type"), str):
                result.append(rule)
    if not result:
        raise ValidationError("GitHub rules payload did not contain any typed rules")
    return result


def normalize_status_contexts(rules: list[dict]) -> list[str]:
    contexts: set[str] = set()
    for rule in rules:
        parameters = rule.get("parameters") or {}
        if not isinstance(parameters, dict):
            continue
        raw_checks = parameters.get("required_status_checks") or []
        if not isinstance(raw_checks, list):
            continue
        for item in raw_checks:
            if not isinstance(item, dict):
                continue
            context = item.get("context")
            if isinstance(context, str) and context:
                contexts.add(context)
    return sorted(contexts)


def bool_field(parameters: dict, name: str) -> bool:
    return bool(parameters.get(name, False))


def expected_bypass_actors(review_policy: dict) -> set[tuple[str, int, str]]:
    enforcement = review_policy.get("enforcement", {})
    bypass_mode = enforcement.get("bypass_mode")
    if bypass_mode == "pull-request-only":
        normalized_mode = "pull_request"
    elif isinstance(bypass_mode, str):
        normalized_mode = bypass_mode.replace("-", "_")
    else:
        raise ValidationError("review_policy.enforcement.bypass_mode must be a string")

    expected: set[tuple[str, int, str]] = set()
    permissions = review_policy.get("exempt_author_permissions", [])
    if not isinstance(permissions, list):
        raise ValidationError("review_policy.exempt_author_permissions must be an array")
    for permission in permissions:
        if not isinstance(permission, str):
            raise ValidationError("review_policy.exempt_author_permissions must contain strings")
        role_id = REPOSITORY_ROLE_IDS.get(permission)
        if role_id is None:
            raise ValidationError(
                f"Unsupported exempt_author_permissions entry for live validation: {permission}"
            )
        expected.add(("RepositoryRole", role_id, normalized_mode))

    if review_policy.get("exempt_repository_owner"):
        expected.add(("RepositoryRole", REPOSITORY_ROLE_IDS["admin"], normalized_mode))

    return expected


def actual_bypass_actors(rulesets: list[dict]) -> set[tuple[str, int, str]]:
    actors: set[tuple[str, int, str]] = set()
    saw_metadata = False
    for ruleset in rulesets:
        raw_actors = ruleset.get("bypass_actors")
        if raw_actors is None:
            raw_actors = ruleset.get("bypassActors")
        if raw_actors is None:
            continue
        saw_metadata = True
        if not isinstance(raw_actors, list):
            raise ValidationError("ruleset bypass_actors must be an array")
        for actor in raw_actors:
            if not isinstance(actor, dict):
                continue
            actor_type = actor.get("actor_type")
            if actor_type is None:
                actor_type = actor.get("actorType")
            actor_id = actor.get("actor_id")
            if actor_id is None:
                actor_id = actor.get("actorId")
            bypass_mode = actor.get("bypass_mode")
            if bypass_mode is None:
                bypass_mode = actor.get("bypassMode")
            normalized_type = actor_type if isinstance(actor_type, str) else ""
            normalized_id = normalize_actor_id(actor_id)
            normalized_mode = normalize_bypass_mode(bypass_mode)
            if normalized_type and normalized_id is not None and normalized_mode:
                actors.add((normalized_type, normalized_id, normalized_mode))
    if not saw_metadata:
        raise ValidationError("GitHub rules payload did not expose ruleset bypass actors")
    return actors


def validate_rules(declaration: dict, rulesets: list[dict], branch: str) -> tuple[list[str], list[dict]]:
    errors: list[str] = []
    rules = flatten_rules(rulesets)
    required_checks = declaration.get("required_checks", [])
    if not isinstance(required_checks, list):
        raise ValidationError("required_checks must be an array")
    required_checks = sorted(item for item in required_checks if isinstance(item, str) and item)

    policy = declaration.get("policy", {})
    branch_policy = policy.get("branch_protection", {})
    review_policy = policy.get("review_policy", {})
    review_enforcement = review_policy.get("enforcement", {})

    require_signed_commits = bool(policy.get("require_signed_commits"))
    require_pull_request = bool(branch_policy.get("require_pull_request"))
    required_approvals = int(review_policy.get("required_approvals", 0))

    grouped: dict[str, list[dict]] = {}
    for rule in rules:
        grouped.setdefault(rule.get("type", ""), []).append(rule)

    if require_signed_commits and "required_signatures" not in grouped:
        errors.append(f"{branch}: missing required_signatures rule")

    if branch_policy.get("disallow_direct_pushes") and "pull_request" not in grouped:
        errors.append(f"{branch}: missing pull_request rule required to block direct pushes")

    if require_pull_request:
        pull_request_rules = grouped.get("pull_request", [])
        if not pull_request_rules:
            errors.append(f"{branch}: missing pull_request rule")
        else:
            max_approvals = 0
            stale_review = False
            code_owner_review = False
            last_push_approval = False
            thread_resolution = False
            merge_method_block = False
            for rule in pull_request_rules:
                parameters = rule.get("parameters") or {}
                if not isinstance(parameters, dict):
                    continue
                value = parameters.get("required_approving_review_count", 0)
                if isinstance(value, bool):
                    value = int(value)
                if isinstance(value, int):
                    max_approvals = max(max_approvals, value)
                stale_review = stale_review or bool_field(parameters, "dismiss_stale_reviews_on_push")
                code_owner_review = code_owner_review or bool_field(parameters, "require_code_owner_review")
                last_push_approval = last_push_approval or bool_field(parameters, "require_last_push_approval")
                thread_resolution = thread_resolution or bool_field(parameters, "required_review_thread_resolution")
                allowed_merge_methods = parameters.get("allowed_merge_methods")
                if isinstance(allowed_merge_methods, list) and allowed_merge_methods:
                    merge_method_block = merge_method_block or ("merge" not in allowed_merge_methods)
            if max_approvals != required_approvals:
                errors.append(
                    f"{branch}: required_approving_review_count={max_approvals} expected={required_approvals}"
                )
            if stale_review:
                errors.append(f"{branch}: dismiss_stale_reviews_on_push must stay disabled")
            if code_owner_review:
                errors.append(f"{branch}: require_code_owner_review must stay disabled")
            if last_push_approval:
                errors.append(f"{branch}: require_last_push_approval must stay disabled")
            if thread_resolution:
                errors.append(f"{branch}: required_review_thread_resolution must stay disabled")
            if merge_method_block:
                errors.append(f"{branch}: merge commits must remain allowed")

    if review_enforcement.get("mode") != "github-native":
        errors.append(f"{branch}: review_policy.enforcement.mode must stay github-native")
    else:
        expected_actors = expected_bypass_actors(review_policy)
        actual_actors = actual_bypass_actors(rulesets)
        missing = sorted(expected_actors - actual_actors)
        unexpected = sorted(actual_actors - expected_actors)
        if missing or unexpected:
            details: list[str] = []
            if missing:
                details.append(
                    "missing=" + ", ".join(f"{actor_type}:{actor_id}:{mode}" for actor_type, actor_id, mode in missing)
                )
            if unexpected:
                details.append(
                    "unexpected="
                    + ", ".join(f"{actor_type}:{actor_id}:{mode}" for actor_type, actor_id, mode in unexpected)
                )
            errors.append(f"{branch}: bypass actor drift ({'; '.join(details)})")

    live_required_checks = normalize_status_contexts(grouped.get("required_status_checks", []))
    if live_required_checks != required_checks:
        missing = sorted(set(required_checks) - set(live_required_checks))
        unexpected = sorted(set(live_required_checks) - set(required_checks))
        details: list[str] = []
        if missing:
            details.append(f"missing={', '.join(missing)}")
        if unexpected:
            details.append(f"unexpected={', '.join(unexpected)}")
        if not details:
            details.append("required status check order/content drifted")
        errors.append(f"{branch}: required_status_checks drift ({'; '.join(details)})")

    return errors, rules


def main() -> int:
    args = parse_args()
    if should_skip(args.mode):
        return 0

    try:
        declaration = load_declaration(args.declaration)
        branch = choose_branch(declaration, args.branch)
        owner, repo = split_repo(args.repo)
        rulesets = extract_rulesets(fetch_json(args.api_root, owner, repo, branch))
        errors, rules = validate_rules(declaration, rulesets, branch)
    except ValidationError as exc:
        print(f"[live-quality-gates] {exc}", file=sys.stderr)
        return 1

    if errors:
        print("[live-quality-gates] drift detected:", file=sys.stderr)
        for item in errors:
            print(f"- {item}", file=sys.stderr)
        return 1

    print(
        json.dumps(
            {
                "status": "ok",
                "repo": args.repo,
                "branch": branch,
                "checked_rules": sorted({rule.get("type", "") for rule in rules}),
                "ruleset_count": len(rulesets),
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
