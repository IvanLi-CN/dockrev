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


class ValidationError(RuntimeError):
    pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate the live GitHub branch rules against .github/quality-gates.json."
    )
    parser.add_argument("--declaration", default=".github/quality-gates.json")
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", ""))
    parser.add_argument("--branch", default="")
    parser.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    parser.add_argument(
        "--mode",
        choices=("auto", "require", "skip"),
        default=os.environ.get("QUALITY_GATES_LIVE_RULES_MODE", "auto"),
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


def _headers() -> dict[str, str]:
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "quality-gates-live-check/2.0",
        "X-GitHub-Api-Version": API_VERSION,
    }
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN") or ""
    if token:
        headers["Authorization"] = f"Bearer {token}"
    return headers


def fetch_branch_rules(api_root: str, owner: str, repo: str, branch: str) -> object:
    path = "/repos/{owner}/{repo}/rules/branches/{branch}?per_page=100".format(
        owner=urllib.parse.quote(owner, safe=""),
        repo=urllib.parse.quote(repo, safe=""),
        branch=urllib.parse.quote(branch, safe=""),
    )
    request = urllib.request.Request(api_root.rstrip("/") + path, headers=_headers())
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        raise ValidationError(f"GitHub API request failed ({exc.code}): {detail}") from exc
    except urllib.error.URLError as exc:
        raise ValidationError(f"GitHub API request failed: {exc.reason}") from exc


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


def required_check_state(declaration: dict, live_required_checks: list[str]) -> str:
    required = sorted(declaration.get("required_checks", []))
    if live_required_checks == required:
        return "target"
    pending = sorted(declaration.get("pending_required_checks", []))
    if pending and live_required_checks == sorted(set(required) - set(pending)):
        return "pending"
    retiring = sorted(declaration.get("retiring_required_checks", []))
    if retiring and live_required_checks == sorted(set(required) | set(retiring)):
        return "retiring"
    return "drift"


def validate_rules(declaration: dict, rules: list[dict], branch: str) -> list[str]:
    errors: list[str] = []
    required_checks = declaration.get("required_checks", [])
    if not isinstance(required_checks, list):
        raise ValidationError("required_checks must be an array")

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

    if branch_policy.get("strict_required_status_checks"):
        status_rules = grouped.get("required_status_checks", [])
        if not status_rules:
            errors.append(f"{branch}: strict required status checks declared without a required_status_checks rule")
        for rule in status_rules:
            parameters = rule.get("parameters") or {}
            if not isinstance(parameters, dict) or not bool_field(parameters, "strict_required_status_checks_policy"):
                errors.append(f"{branch}: strict_required_status_checks_policy must stay enabled")
                break

    if branch_policy.get("require_extra_approval_for_unattributed_changes"):
        pull_request_rules = grouped.get("pull_request", [])
        if not pull_request_rules:
            errors.append(f"{branch}: extra approval for unattributed changes declared without a pull_request rule")
        elif not any(
            bool_field(rule.get("parameters") or {}, "require_extra_approval_for_unattributed_changes")
            for rule in pull_request_rules
        ):
            errors.append(f"{branch}: require_extra_approval_for_unattributed_changes must stay enabled")

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
    state = required_check_state(declaration, live_required_checks)
    if state == "drift":
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
        states: dict[str, str] = {}
        for branch in branches:
            rules = extract_rules(fetch_branch_rules(args.api_root, owner, repo, branch))
            checked_rules[branch] = sorted({rule.get("type", "") for rule in rules})
            errors.extend(validate_rules(declaration, rules, branch))
            states[branch] = required_check_state(
                declaration, normalize_status_contexts([rule for rule in rules if rule.get("type") == "required_status_checks"])
            )
            if states[branch] == "pending":
                print(
                    "[live-quality-gates] declared checks not yet required by the live ruleset: "
                    + ", ".join(sorted(declaration.get("pending_required_checks", []))),
                    file=sys.stderr,
                )
            if states[branch] == "retiring":
                print(
                    "[live-quality-gates] checks still required by the live ruleset but declared for removal: "
                    + ", ".join(sorted(declaration.get("retiring_required_checks", []))),
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
        "Bypass actors are not exposed by that endpoint and must be verified during live ruleset configuration.",
    ]
    if "pending" in states.values():
        notes.append("Pending required checks are declared but not yet enforced by the live ruleset.")
    if "retiring" in states.values():
        notes.append("Retiring required checks are still enforced by the live ruleset; remove them from the ruleset.")
    print(
        json.dumps(
            {
                "status": "transition_in_progress" if {"pending", "retiring"} & set(states.values()) else "ok",
                "repo": args.repo,
                "branches": branches,
                "checked_rules": checked_rules,
                "required_check_state": states,
                "notes": notes,
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
