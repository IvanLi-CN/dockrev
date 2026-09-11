#!/usr/bin/env python3
"""Fail-closed policy primitives for the PR label release contract."""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.parse
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
POLICY_PATH = ROOT / ".github/pr-label-release.json"
VERSION_RE = re.compile(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$")
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
CHANNEL_PRERELEASE_RE = {
    "beta": re.compile(r"beta\.[0-9]+"),
    "rc": re.compile(r"rc\.[0-9]+"),
    "dev": re.compile(r"dev\.[0-9]+"),
}
UNTRUSTED_SOURCE_PATH_PREFIXES = (
    ".github/workflows/",
    ".github/scripts/release_",
    ".github/scripts/check-live-quality-gates.py",
    ".github/scripts/label-gate.sh",
    ".github/scripts/release-channel-contract-check.sh",
    ".github/quality-gates.json",
    ".github/release-failure-notification.json",
)


class PolicyError(ValueError):
    pass


def load_policy(path: Path = POLICY_PATH) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("schema_version") != 2:
        raise PolicyError("unsupported PR label release policy schema")
    return payload


def _allowed(policy: dict[str, Any], group: str) -> set[str]:
    for item in policy["label_groups"]:
        if item["name"] == group:
            return set(item["allowed"])
    raise PolicyError(f"missing label group: {group}")


def parse_labels(labels: list[str], policy: dict[str, Any] | None = None) -> dict[str, Any]:
    policy = policy or load_policy()
    values = [str(label).strip() for label in labels if str(label).strip()]
    type_allowed = _allowed(policy, "type")
    channel_allowed = _allowed(policy, "channel")
    component_allowed = _allowed(policy, "component")
    groups = {
        "type": [value for value in values if value.startswith("type:")],
        "channel": [value for value in values if value.startswith("channel:")],
        "component": [value for value in values if value.startswith("component:")],
    }
    errors: list[str] = []
    for group, group_values in groups.items():
        allowed = {"type": type_allowed, "channel": channel_allowed, "component": component_allowed}[group]
        unknown = sorted(set(group_values) - allowed)
        if unknown:
            errors.append(f"unknown {group} label(s): {', '.join(unknown)}")
        if group in {"type", "channel"} and len(group_values) != len(set(group_values)):
            errors.append(f"duplicate {group} label(s)")
    if len(groups["type"]) != 1:
        errors.append(f"exactly one type:* label required; found {len(groups['type'])}")
    if len(groups["channel"]) != 1:
        errors.append(f"exactly one channel:* label required; found {len(groups['channel'])}")
    if errors:
        raise PolicyError("; ".join(errors))
    type_label = groups["type"][0]
    channel_label = groups["channel"][0]
    return {
        "type": type_label.removeprefix("type:"),
        "channel": channel_label.removeprefix("channel:"),
        "type_label": type_label,
        "channel_label": channel_label,
        "components": sorted(set(value.removeprefix("component:") for value in groups["component"])),
        "labels": sorted(set(values)),
        "release_enabled": type_label != "type:none",
    }


def parse_version(version: str) -> tuple[int, int, int, str | None]:
    match = VERSION_RE.fullmatch(version.strip())
    if not match:
        raise PolicyError(f"invalid VERSION: {version!r}")
    return int(match.group(1)), int(match.group(2)), int(match.group(3)), match.group(4)


def next_patch(version: str) -> str:
    major, minor, patch, prerelease = parse_version(version)
    if prerelease:
        raise PolicyError("automatic patch cannot derive from a prerelease VERSION")
    return f"{major}.{minor}.{patch + 1}"


def channel_for_version(version: str) -> str:
    _major, _minor, _patch, prerelease = parse_version(version)
    if prerelease is None:
        return "stable"
    for channel, pattern in CHANNEL_PRERELEASE_RE.items():
        if pattern.fullmatch(prerelease):
            return channel
    raise PolicyError(f"VERSION prerelease does not identify a supported release channel: {version!r}")


def patch_channel_transitions() -> dict[str, set[str]]:
    policy = load_policy()
    contract = policy.get("promotion_contract")
    if not isinstance(contract, dict) or contract.get("type") != "patch-only":
        raise PolicyError("missing patch-only promotion contract")
    transitions = contract.get("transitions")
    if not isinstance(transitions, dict):
        raise PolicyError("promotion contract must define channel transitions")
    channels = {label.removeprefix("channel:") for label in _allowed(policy, "channel")}
    if set(transitions) != channels:
        raise PolicyError("promotion contract must define transitions for every release channel")
    normalized: dict[str, set[str]] = {}
    for source, targets in transitions.items():
        if not isinstance(targets, list) or not targets or set(targets) - channels:
            raise PolicyError(f"invalid promotion targets for {source!r}")
        normalized[source] = set(targets)
    return normalized


def validate_preparation_version(source_version: str, version: str, intent: dict[str, Any]) -> None:
    source = parse_version(source_version)
    target = parse_version(version)
    source_channel = channel_for_version(source_version)
    target_channel = str(intent["channel"])
    if intent["type"] == "patch":
        expected_base = source[:3] if source_channel != "stable" else (source[0], source[1], source[2] + 1)
        if target[:3] != expected_base:
            raise PolicyError("patch preparation must advance VERSION by exactly one patch base")
        transitions = patch_channel_transitions()
        if target_channel not in transitions[source_channel]:
            raise PolicyError(
                f"patch release promotion from {source_channel} to {target_channel} is not allowed"
            )
        if source_channel == "stable" and target_channel == "stable" and version != next_patch(source_version):
            raise PolicyError("stable patch preparation must use the next patch")
        if source_channel == target_channel and source_channel != "stable":
            source_sequence = int(str(source[3]).rsplit(".", 1)[1])
            target_sequence = int(str(target[3]).rsplit(".", 1)[1])
            if target_sequence <= source_sequence:
                raise PolicyError("prerelease patch preparation must advance its channel sequence")
    elif intent["type"] == "minor" and target[:2] <= source[:2]:
        raise PolicyError("minor preparation must advance the source major/minor")
    elif intent["type"] == "major" and target[0] <= source[0]:
        raise PolicyError("major preparation must advance the source major")
    validate_channel_version(version, target_channel)


def validate_channel_version(version: str, channel: str) -> None:
    if channel not in {"stable", *CHANNEL_PRERELEASE_RE}:
        raise PolicyError(f"unsupported release channel: {channel}")
    version_channel = channel_for_version(version)
    if version_channel != channel:
        suffix = "a final semver VERSION" if channel == "stable" else f"VERSION suffix -{channel}.N"
        raise PolicyError(f"{channel} channel requires {suffix}")


def validate_source_boundary(changed_files: list[str]) -> None:
    """Reject product PRs that can change the workflow used as their evidence."""
    forbidden = sorted(
        path for path in changed_files
        if path == ".github/pr-label-release.json"
        or any(path.startswith(prefix) for prefix in UNTRUSTED_SOURCE_PATH_PREFIXES)
    )
    if forbidden:
        raise PolicyError("source CI evidence is untrusted when release workflow files change: " + ", ".join(forbidden))


def validate_sha(value: str, field: str = "sha") -> None:
    if not SHA_RE.fullmatch(value):
        raise PolicyError(f"{field} must be a 40-character lowercase commit SHA")


def parse_trailers(message: str) -> dict[str, str]:
    trailers: dict[str, str] = {}
    for line in message.splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        if key in {"Source-SHA", "Source-PR-Updated-At", "Product-Version", "Release-Intent", "Release-Mode", "Covered-Product-Merge-SHA"}:
            if key in trailers:
                raise PolicyError(f"duplicate release trailer: {key}")
            trailers[key] = value.strip()
    return trailers


def parse_reservation_trailers(message: str) -> dict[str, str]:
    trailers: dict[str, str] = {}
    keys = {
        "Release-Reservation-Version",
        "Release-Reservation-PR",
        "Release-Reservation-Source-SHA",
        "Release-Reservation-Identity-SHA",
        "Release-Reservation-Intent",
        "Release-Reservation-Mode",
    }
    for line in message.splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        if key in keys:
            if key in trailers:
                raise PolicyError(f"duplicate reservation trailer: {key}")
            trailers[key] = value.strip()
    return trailers


def validate_reservation(
    payload: dict[str, Any], *, version: str, pr_number: int, source_sha: str
) -> dict[str, Any]:
    message = payload.get("message") or payload.get("commit", {}).get("message", "")
    trailers = parse_reservation_trailers(str(message))
    if trailers.get("Release-Reservation-Version") != version:
        raise PolicyError("release reservation version does not match expected VERSION")
    if trailers.get("Release-Reservation-PR") != str(pr_number):
        raise PolicyError("release reservation belongs to another PR")
    if trailers.get("Release-Reservation-Source-SHA") != source_sha:
        raise PolicyError("release reservation source SHA does not match expected source")
    validate_sha(source_sha, "reservation_source_sha")
    parents = [parent.get("sha") for parent in payload.get("parents", [])]
    if parents != [source_sha]:
        raise PolicyError("release reservation must have the source SHA as its only parent")
    return payload


def validate_version_only_reservation(
    payload: dict[str, Any], *, version: str, pr_number: int, source_sha: str
) -> dict[str, str]:
    validate_reservation(payload, version=version, pr_number=pr_number, source_sha=source_sha)
    message = payload.get("message") or payload.get("commit", {}).get("message", "")
    trailers = parse_reservation_trailers(str(message))
    identity_sha = trailers.get("Release-Reservation-Identity-SHA", "")
    validate_sha(identity_sha, "version-only reservation identity SHA")
    if trailers.get("Release-Reservation-Mode") != "version-only-release-pr":
        raise PolicyError("release reservation mode is not version-only-release-pr")
    try:
        intent = parse_labels(trailers.get("Release-Reservation-Intent", "").split())
    except PolicyError as error:
        raise PolicyError("version-only reservation intent is invalid") from error
    if not intent["release_enabled"]:
        raise PolicyError("version-only reservation intent must be release-enabled")
    return trailers


def validate_preparation(payload: dict[str, Any], *, source_sha: str | None = None) -> dict[str, Any]:
    required = {"commit_sha", "source_sha", "source_pr_updated_at", "version", "intent", "release_mode", "parents", "changed_files", "verified"}
    missing = sorted(required - set(payload))
    if missing:
        raise PolicyError(f"preparation provenance missing: {', '.join(missing)}")
    validate_sha(str(payload["commit_sha"]), "commit_sha")
    validate_sha(str(payload["source_sha"]), "source_sha")
    if not re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z", str(payload["source_pr_updated_at"])):
        raise PolicyError("preparation source_pr_updated_at is not an RFC3339 UTC timestamp")
    if source_sha and payload["source_sha"] != source_sha:
        raise PolicyError("preparation source_sha does not match expected source")
    parse_version(str(payload["version"]))
    if payload["release_mode"] != "normal-preparation":
        raise PolicyError("unexpected preparation release mode")
    if payload["parents"] != [payload["source_sha"]]:
        raise PolicyError("preparation must have exactly the source SHA as its only parent")
    if payload["changed_files"] != ["VERSION"]:
        raise PolicyError("preparation commit must change VERSION only")
    if payload["verified"] is not True:
        raise PolicyError("preparation commit signature is not verified")
    return payload


def validate_version_only(files: list[str], provenance: dict[str, Any], *, head_sha: str | None = None) -> None:
    if not files or files != ["VERSION"]:
        raise PolicyError("version-only release PR must be non-empty and change VERSION only")
    required = {
        "covered_product_merge_sha",
        "covered_product_pr_number",
        "covered_product_version",
        "covered_product_merged",
        "product_version",
        "release_intent",
        "release_mode",
        "verified",
        "branch_head_sha",
    }
    missing = sorted(required - set(provenance))
    if missing:
        raise PolicyError(f"version-only provenance missing: {', '.join(missing)}")
    validate_sha(str(provenance["covered_product_merge_sha"]), "covered_product_merge_sha")
    if not isinstance(provenance["covered_product_pr_number"], int) or provenance["covered_product_pr_number"] < 1:
        raise PolicyError("covered_product_pr_number must identify one product PR")
    if provenance["covered_product_merged"] is not True:
        raise PolicyError("covered product boundary is not a merged PR")
    validate_sha(str(provenance["branch_head_sha"]), "branch_head_sha")
    if head_sha and provenance["branch_head_sha"] != head_sha:
        raise PolicyError("version-only branch head drifted")
    if provenance["verified"] is not True:
        raise PolicyError("version-only release PR signature is not verified")
    covered_product_version = str(provenance["covered_product_version"])
    parse_version(covered_product_version)
    parse_version(str(provenance["product_version"]))
    try:
        intent = parse_labels(str(provenance["release_intent"]).split())
    except PolicyError as error:
        raise PolicyError("version-only release intent is invalid") from error
    if not intent["release_enabled"]:
        raise PolicyError("version-only release PR requires a release-enabled type")
    try:
        validate_channel_version(str(provenance["product_version"]), intent["channel"])
        validate_preparation_version(covered_product_version, str(provenance["product_version"]), intent)
    except PolicyError as error:
        raise PolicyError("version-only release VERSION is incompatible with the covered product identity") from error
    if provenance["release_mode"] != "version-only-release-pr":
        raise PolicyError("version-only release PR has invalid release mode")


def validate_identity(payload: dict[str, Any]) -> dict[str, Any]:
    required = {"merge_commit_sha", "version", "release_tag", "release_mode", "intent", "source_sha"}
    missing = sorted(required - set(payload))
    if missing:
        raise PolicyError(f"release identity missing: {', '.join(missing)}")
    validate_sha(str(payload["merge_commit_sha"]), "merge_commit_sha")
    validate_sha(str(payload["source_sha"]), "source_sha")
    parse_version(str(payload["version"]))
    if payload["release_tag"] != f"v{payload['version']}":
        raise PolicyError("release tag must be derived from VERSION only")
    if payload["release_mode"] not in {"normal-preparation", "version-only-release-pr"}:
        raise PolicyError("release identity mode is not publishable")
    if payload["release_mode"] == "normal-preparation" and payload.get("covered_product_merge_sha"):
        raise PolicyError("normal-preparation identity cannot carry Covered-Product-Merge-SHA")
    if payload["release_mode"] == "version-only-release-pr" and payload.get("preparation_commit_sha"):
        raise PolicyError("version-only identity cannot carry preparation_commit_sha")
    return payload


def validate_failure_context(
    payload: dict[str, Any], *, expected_repository: str | None = None, expected_run_id: str | None = None,
    expected_server: str | None = None, expected_attempt: str | None = None
) -> dict[str, Any]:
    required = {"pull_request", "source_sha", "merge_commit_sha", "type", "channel", "version", "tag", "artifact_names", "run_url", "recovery_instruction"}
    missing = sorted(required - set(payload))
    if missing:
        raise PolicyError(f"failure context missing: {', '.join(missing)}")
    if not isinstance(payload["artifact_names"], list) or not payload["artifact_names"]:
        raise PolicyError("failure context artifact_names must be non-empty")
    validate_sha(str(payload["source_sha"]), "source_sha")
    validate_sha(str(payload["merge_commit_sha"]), "merge_commit_sha")
    parse_version(str(payload["version"]))
    failure_kind = payload.get("identity_failure_kind")
    if failure_kind == "resolver-error":
        if payload["type"] != "unknown" or payload["channel"] != "unknown":
            raise PolicyError("resolver-error context must use unknown intent")
    else:
        if payload["type"] not in {"major", "minor", "patch"}:
            raise PolicyError("failure context type is not release-enabled")
        validate_channel_version(str(payload["version"]), str(payload["channel"]))
    if payload["tag"] != f"v{payload['version']}":
        raise PolicyError("failure context tag does not match VERSION")
    run_url = str(payload["run_url"])
    parsed_run_url = urllib.parse.urlparse(run_url)
    if parsed_run_url.scheme != "https" or not parsed_run_url.netloc or not re.fullmatch(r"/[^/]+/[^/]+/actions/runs/[0-9]+", parsed_run_url.path):
        raise PolicyError("failure context run_url must be an absolute HTTPS URL")
    if expected_repository and expected_run_id:
        expected_path = f"/{expected_repository}/actions/runs/{expected_run_id}"
        if parsed_run_url.path != expected_path:
            raise PolicyError("failure context run_url is not bound to the triggering Release run")
        if payload.get("repository") != expected_repository:
            raise PolicyError("failure context repository is not bound to the triggering repository")
    if expected_server and parsed_run_url.netloc != expected_server.removeprefix("https://"):
        raise PolicyError("failure context run_url host is not the configured GitHub server")
    if expected_attempt and str(payload.get("run_attempt")) != str(expected_attempt):
        raise PolicyError("failure context attempt is not bound to the triggering Release attempt")
    recovery = str(payload["recovery_instruction"])
    if failure_kind is not None and failure_kind not in {"no-identity", "resolver-error", "identity-step-failure"}:
        raise PolicyError("failure context identity_failure_kind is unsupported")
    identity_failed = payload.get("identity_resolution_failed")
    if identity_failed is not None and not isinstance(identity_failed, bool):
        raise PolicyError("failure context identity_resolution_failed must be boolean")
    if failure_kind == "no-identity" and identity_failed is not True:
        raise PolicyError("no-identity failure context must set identity_resolution_failed")
    if failure_kind == "resolver-error" and identity_failed is True:
        raise PolicyError("resolver-error cannot be marked as missing identity")
    if failure_kind == "identity-step-failure" and identity_failed is not False:
        raise PolicyError("identity-step-failure must preserve a resolved identity")
    if payload.get("identity_resolution_failed") is True or failure_kind == "no-identity":
        expected_recovery = f"create VERSION-only release PR Covered-Product-Merge-SHA={payload['merge_commit_sha']}"
    elif failure_kind == "resolver-error":
        expected_recovery = "resolver-error: retry Release workflow after verifying merged identity"
    elif failure_kind == "identity-step-failure":
        expected_recovery = f"workflow_dispatch merge_sha={payload['merge_commit_sha']} recovery_reason=<required>"
    else:
        expected_recovery = f"workflow_dispatch merge_sha={payload['merge_commit_sha']} recovery_reason=<required>"
    if recovery != expected_recovery:
        raise PolicyError("failure context recovery instruction is not bound to its remediation boundary")
    if payload.get("identity_resolution_failed") is True and payload["source_sha"] != payload["merge_commit_sha"]:
        raise PolicyError("identity-resolution fallback must use the merge SHA as source evidence")
    if (
        failure_kind is None
        and identity_failed is not True
        and payload["source_sha"] == payload["merge_commit_sha"]
    ):
        raise PolicyError("merge-SHA failure context requires an explicit identity failure classification")
    return payload


def _cli() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    labels = sub.add_parser("labels")
    labels.add_argument("--labels-json", required=True)
    patch = sub.add_parser("next-patch")
    patch.add_argument("--version", required=True)
    channel = sub.add_parser("validate-channel")
    channel.add_argument("--version", required=True)
    channel.add_argument("--channel", required=True)
    resolved_channel = sub.add_parser("channel-for-version")
    resolved_channel.add_argument("--version", required=True)
    prep = sub.add_parser("validate-preparation")
    prep.add_argument("--input", type=Path, required=True)
    version_only = sub.add_parser("validate-version-only")
    version_only.add_argument("--files", type=Path, required=True)
    version_only.add_argument("--provenance", type=Path, required=True)
    identity = sub.add_parser("validate-identity")
    identity.add_argument("--input", type=Path, required=True)
    failure = sub.add_parser("validate-failure")
    failure.add_argument("--input", type=Path, required=True)
    return parser


def main() -> int:
    args = _cli().parse_args()
    try:
        if args.command == "labels":
            print(json.dumps(parse_labels(json.loads(args.labels_json)), sort_keys=True))
        elif args.command == "next-patch":
            print(next_patch(args.version))
        elif args.command == "validate-channel":
            validate_channel_version(args.version, args.channel)
            print("ok")
        elif args.command == "channel-for-version":
            print(channel_for_version(args.version))
        elif args.command == "validate-preparation":
            validate_preparation(json.loads(args.input.read_text(encoding="utf-8")))
            print("ok")
        elif args.command == "validate-version-only":
            validate_version_only(json.loads(args.files.read_text(encoding="utf-8")), json.loads(args.provenance.read_text(encoding="utf-8")))
            print("ok")
        elif args.command == "validate-identity":
            validate_identity(json.loads(args.input.read_text(encoding="utf-8")))
            print("ok")
        elif args.command == "validate-failure":
            validate_failure_context(json.loads(args.input.read_text(encoding="utf-8")))
            print("ok")
        return 0
    except (PolicyError, json.JSONDecodeError, OSError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
