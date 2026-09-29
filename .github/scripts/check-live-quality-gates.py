#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

API_VERSION = "2022-11-28"


class ValidationError(RuntimeError):
    pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate the live GitHub branch rules against .github/quality-gates.json."
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
        help="Protected branch to validate. Defaults to all protected branches declared in the contract.",
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


def choose_branches(declaration: dict, override: str) -> list[str]:
    if override:
        return [override]
    raw_branches = (
        declaration.get("policy", {})
        .get("branch_protection", {})
        .get("protected_branches", [])
    )
    if not isinstance(raw_branches, list) or not raw_branches:
        raise ValidationError("protected_branches must declare at least one protected branch")
    branches: list[str] = []
    for index, branch in enumerate(raw_branches):
        if not isinstance(branch, str) or not branch:
            raise ValidationError(f"protected_branches[{index}] must be a non-empty string")
        if branch not in branches:
            branches.append(branch)
    return branches


def split_repo(repo: str) -> tuple[str, str]:
    parts = repo.split("/", 1)
    if len(parts) != 2 or not parts[0] or not parts[1]:
        raise ValidationError("--repo must be in owner/name form")
    return parts[0], parts[1]


def fetch_branch_rules(api_root: str, owner: str, repo: str, branch: str) -> object:
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


def fetch_branch_file(api_root: str, owner: str, repo: str, branch: str, path: str) -> str | None:
    quoted_path = urllib.parse.quote(path, safe="/")
    quoted_branch = urllib.parse.quote(branch, safe="")
    url = api_root.rstrip("/") + f"/repos/{owner}/{repo}/contents/{quoted_path}?ref={quoted_branch}"
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
            payload = json.load(response)
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        if exc.code == 404:
            return None
        raise ValidationError(f"GitHub API request failed ({exc.code}): {detail}") from exc
    except urllib.error.URLError as exc:
        raise ValidationError(f"GitHub API request failed: {exc.reason}") from exc

    if not isinstance(payload, dict) or payload.get("encoding") != "base64":
        raise ValidationError(f"{branch}:{path} is not a base64 GitHub contents response")
    try:
        content = "".join(str(payload["content"]).split())
        return base64.b64decode(content, validate=True).decode("utf-8")
    except (KeyError, ValueError, UnicodeDecodeError) as exc:
        raise ValidationError(f"{branch}:{path} has invalid encoded content") from exc


def extract_rules(payload: object) -> list[dict]:
    if isinstance(payload, dict) and isinstance(payload.get("data"), list):
        payload = payload["data"]

    if not isinstance(payload, list):
        raise ValidationError("Unsupported GitHub branch rules payload type")

    rules = [item for item in payload if isinstance(item, dict) and isinstance(item.get("type"), str)]
    if not rules:
        raise ValidationError("GitHub branch rules payload did not contain any typed rules")
    return rules


def bool_field(parameters: dict, name: str) -> bool:
    return bool(parameters.get(name, False))


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


def required_check_migration_state(declaration: dict, live_required_checks: list[str]) -> str:
    target = sorted(declaration.get("required_checks", []))
    if live_required_checks == target:
        return "target"
    migration = declaration.get("required_check_migration", {})
    source = migration.get("source_required_checks", []) if isinstance(migration, dict) else []
    if live_required_checks == sorted(source):
        return "source"
    return "drift"


def target_workflow_status(declaration: dict, content: str | None) -> str:
    migration = declaration.get("required_check_migration", {})
    workflow = migration.get("target_workflow", {}) if isinstance(migration, dict) else {}
    markers = workflow.get("trusted_markers", []) if isinstance(workflow, dict) else []
    if content is None:
        return "missing"
    if not isinstance(markers, list) or not markers or not all(
        isinstance(marker, str) and marker and marker in content for marker in markers
    ):
        return "untrusted"
    return "trusted"


def migration_readiness_record(
    state: str, workflow_status: str, check_context: str, live_checks: list[str],
) -> dict[str, str | bool]:
    context_required = check_context in live_checks
    return {
        "state": state,
        "trusted_target_workflow_on_main": workflow_status == "trusted",
        "target_context_required": context_required,
        "target_gate_active": state == "target" and context_required,
    }


def validate_rules(declaration: dict, rules: list[dict], branch: str) -> list[str]:
    errors: list[str] = []
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
    require_merge_queue = bool(branch_policy.get("require_merge_queue", False))
    required_approvals = int(review_policy.get("required_approvals", 0))
    enforcement_mode = review_enforcement.get("mode")
    expected_native_approvals = required_approvals if enforcement_mode == "github-native" else 0

    grouped: dict[str, list[dict]] = {}
    for rule in rules:
        grouped.setdefault(rule.get("type", ""), []).append(rule)

    if require_signed_commits and "required_signatures" not in grouped:
        errors.append(f"{branch}: missing required_signatures rule")

    if require_merge_queue and "merge_queue" not in grouped:
        errors.append(f"{branch}: missing merge_queue rule")
    if not require_merge_queue and "merge_queue" in grouped:
        errors.append(f"{branch}: unexpected merge_queue rule")

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
            if max_approvals != expected_native_approvals:
                errors.append(
                    f"{branch}: required_approving_review_count={max_approvals} expected={expected_native_approvals}"
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

    if enforcement_mode not in {"github-native", "required-check"}:
        errors.append(f"{branch}: unsupported review_policy.enforcement.mode={enforcement_mode!r}")
    elif enforcement_mode == "github-native":
        if review_enforcement.get("bypass_mode") != "pull-request-only":
            errors.append(f"{branch}: review_policy bypass must stay pull-request-only")
    else:
        check_name = review_enforcement.get("check_name")
        if not isinstance(check_name, str) or not check_name:
            errors.append(f"{branch}: review_policy.enforcement.check_name must be set for required-check mode")

    live_required_checks = normalize_status_contexts(grouped.get("required_status_checks", []))
    if required_check_migration_state(declaration, live_required_checks) == "drift":
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

    return errors


def main() -> int:
    args = parse_args()
    if should_skip(args.mode):
        return 0

    try:
        declaration = load_declaration(args.declaration)
        branches = choose_branches(declaration, args.branch)
        owner, repo = split_repo(args.repo)
        errors: list[str] = []
        checked_rules: dict[str, list[str]] = {}
        migration_states: dict[str, str] = {}
        migration_readiness: dict[str, dict[str, str | bool]] = {}
        for branch in branches:
            rules = extract_rules(fetch_branch_rules(args.api_root, owner, repo, branch))
            checked_rules[branch] = sorted({rule.get("type", "") for rule in rules})
            errors.extend(validate_rules(declaration, rules, branch))
            status_rules = [rule for rule in rules if rule.get("type") == "required_status_checks"]
            live_checks = normalize_status_contexts(status_rules)
            migration_states[branch] = required_check_migration_state(declaration, live_checks)
            migration = declaration.get("required_check_migration", {})
            target_workflow = migration.get("target_workflow", {})
            if not isinstance(target_workflow, dict):
                raise ValidationError("required_check_migration.target_workflow must be an object")
            workflow_path = target_workflow.get("path")
            workflow_branch = target_workflow.get("branch")
            check_context = target_workflow.get("check_context")
            if not all(isinstance(value, str) and value for value in (workflow_path, workflow_branch, check_context)):
                raise ValidationError("target workflow branch, path, and check_context must be set")
            workflow_content = fetch_branch_file(
                args.api_root, owner, repo, workflow_branch, workflow_path
            )
            workflow_status = target_workflow_status(declaration, workflow_content)
            target_checks = sorted(migration.get("target_required_checks", []))
            check_declared = check_context in target_checks
            if not check_declared:
                errors.append(f"{branch}: target check context is missing from target_required_checks")
            if migration_states[branch] == "target" and workflow_status != "trusted":
                errors.append(
                    f"{branch}: target required checks are active but trusted workflow is {workflow_status} on {workflow_branch}"
                )
            migration_readiness[branch] = migration_readiness_record(
                migration_states[branch], workflow_status, check_context, live_checks
            )
            if migration_states[branch] == "source":
                print(
                    f"[live-quality-gates] {branch}: legacy required checks remain active; "
                    f"trusted target workflow status on {workflow_branch} is {workflow_status}; "
                    "the Manual Version Release Completion cutover remains pending",
                    file=sys.stderr,
                )
    except ValidationError as exc:
        print(f"[live-quality-gates] {exc}", file=sys.stderr)
        return 1

    if errors:
        print("[live-quality-gates] drift detected:", file=sys.stderr)
        for item in errors:
            print(f"- {item}", file=sys.stderr)
        return 1

    notes = [
        "Validated effective branch rules via GET /repos/{owner}/{repo}/rules/branches/{branch}.",
        "Validated the target workflow markers from its trusted branch.",
        "Bypass actors are not exposed by that endpoint and must be verified during live ruleset configuration.",
    ]
    if "source" in migration_states.values():
        notes.append("The source state is pending; the target check is not required by the live ruleset yet.")
    if "target" in migration_states.values():
        notes.append("The active target context must succeed on each PR before GitHub permits merge.")

    print(
        json.dumps(
            {
                "status": (
                    "transition_in_progress"
                    if "source" in migration_states.values()
                    else "ok"
                ),
                "repo": args.repo,
                "branches": branches,
                "checked_rules": checked_rules,
                "required_check_migration": migration_states,
                "required_check_migration_readiness": migration_readiness,
                "notes": notes,
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
