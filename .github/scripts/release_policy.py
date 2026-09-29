#!/usr/bin/env python3
"""Manual version allocation and immutable release identity contracts."""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.parse
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
POLICY_PATH = ROOT / ".github/manual-version-release.json"
VERSION_RE = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-(alpha|beta|rc)\.(0|[1-9][0-9]*))?$"
)
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
TRAILER_KEYS = {
    "Source-SHA",
    "Product-Version",
    "Release-Baseline-Version",
    "Release-Intent",
}


class PolicyError(ValueError):
    pass


def load_policy(path: Path = POLICY_PATH) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PolicyError(f"manual version release policy is unavailable: {error}") from error
    expected = {
        "schema_version": 1,
        "baseline": {"ref": "main", "file": "VERSION"},
        "intents": ["major", "minor", "patch", "alpha", "beta", "rc"],
        "exact_version": {
            "stable": "X.Y.Z",
            "prerelease": ["X.Y.Z-alpha.N", "X.Y.Z-beta.N", "X.Y.Z-rc.N"],
            "build_metadata": False,
            "v_prefix": False,
        },
        "prerelease_channels": ["alpha", "beta", "rc"],
        "prerelease_sequence_start": 1,
        "prerelease_transitions": [
            "alpha:alpha,beta",
            "beta:beta,rc",
            "rc:rc,stable",
        ],
        "identity": {
            "changed_files": ["VERSION"],
            "signed": True,
            "immutable_version_reservation": True,
            "preparation_branch_prefix": "release-preparation/v",
            "reservation_branch_prefix": "release-reservation/v",
        },
        "publication": {
            "stable": {
                "github_release": True,
                "ghcr_version": True,
                "ghcr_latest": True,
            },
            "prerelease": {
                "github_prerelease": True,
                "ghcr_version": True,
                "ghcr_latest": False,
            },
        },
    }
    if payload != expected:
        raise PolicyError("manual version release policy differs from the enforced contract")
    return payload


def parse_version(version: str) -> tuple[int, int, int, str | None, int | None]:
    match = VERSION_RE.fullmatch(version)
    if not match:
        raise PolicyError(f"invalid SemVer VERSION: {version!r}")
    suffix = match.group(4)
    return (
        int(match.group(1)),
        int(match.group(2)),
        int(match.group(3)),
        suffix,
        int(match.group(5)) if match.group(5) is not None else None,
    )


def parse_version_file(contents: str) -> str:
    version = contents[:-1] if contents.endswith("\n") else contents
    if not version or "\n" in version or "\r" in version:
        raise PolicyError("VERSION file must contain exactly one SemVer line")
    parse_version(version)
    return version


def channel_for_version(version: str) -> str:
    return parse_version(version)[3] or "stable"


def validate_channel_version(version: str, channel: str) -> None:
    if channel not in {"stable", "alpha", "beta", "rc"}:
        raise PolicyError(f"unsupported release channel: {channel}")
    if channel_for_version(version) != channel:
        raise PolicyError(f"VERSION is not on the {channel} channel")


def validate_sha(value: str, field: str = "sha") -> None:
    if not SHA_RE.fullmatch(value):
        raise PolicyError(f"{field} must be a 40-character lowercase commit SHA")


def next_core(version: str, intent: str) -> tuple[int, int, int]:
    major, minor, patch, _channel, _sequence = parse_version(version)
    if intent == "major":
        return major + 1, 0, 0
    if intent == "minor":
        return major, minor + 1, 0
    if intent == "patch":
        return major, minor, patch + 1
    raise PolicyError(f"unsupported numeric version intent: {intent}")


def _pre_release_target(baseline: str, channel: str) -> str:
    major, minor, patch, current_channel, sequence = parse_version(baseline)
    if current_channel is None and channel in {"alpha", "beta"}:
        core = (major, minor, patch + 1)
        number = 1
    elif current_channel == "alpha" and channel == "alpha":
        core = (major, minor, patch)
        number = int(sequence or 0) + 1
    elif current_channel == "alpha" and channel == "beta":
        core = (major, minor, patch)
        number = 1
    elif current_channel == "beta" and channel == "beta":
        core = (major, minor, patch)
        number = int(sequence or 0) + 1
    elif current_channel == "beta" and channel == "rc":
        core = (major, minor, patch)
        number = 1
    elif current_channel == "rc" and channel == "rc":
        core = (major, minor, patch)
        number = int(sequence or 0) + 1
    else:
        raise PolicyError(f"cannot start or move to {channel} from {current_channel}")
    return f"{core[0]}.{core[1]}.{core[2]}-{channel}.{number}"


def compute_target(baseline: str, version_input: str) -> dict[str, str]:
    parse_version(baseline)
    if not version_input or version_input != version_input.strip():
        raise PolicyError("version input must be a non-empty canonical value")
    if version_input in {"major", "minor", "patch"}:
        core = next_core(baseline, version_input)
        target = f"{core[0]}.{core[1]}.{core[2]}"
    elif version_input in {"alpha", "beta", "rc"}:
        target = _pre_release_target(baseline, version_input)
    else:
        target = version_input
        parse_version(target)
        target_channel = channel_for_version(target)
        baseline_channel = channel_for_version(baseline)
        core = parse_version(target)[:3]
        if target_channel == "stable":
            numeric_targets = {
                f"{parts[0]}.{parts[1]}.{parts[2]}"
                for intent in ("major", "minor", "patch")
                for parts in (next_core(baseline, intent),)
            }
            promoted = baseline_channel == "rc" and core == parse_version(baseline)[:3]
            if target not in numeric_targets and not promoted:
                raise PolicyError("exact stable version must match a calculated target or promote the current rc")
        else:
            expected = _pre_release_target(baseline, target_channel)
            if target != expected:
                raise PolicyError(f"exact prerelease version must equal the next valid identity: {expected}")
    validate_channel_version(target, channel_for_version(target))
    return {
        "baseline_version": baseline,
        "version_input": version_input,
        "version": target,
        "channel": channel_for_version(target),
    }


def parse_trailers(message: str) -> dict[str, str]:
    trailers: dict[str, str] = {}
    for line in message.splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        if key in TRAILER_KEYS:
            if key in trailers:
                raise PolicyError(f"duplicate release trailer: {key}")
            trailers[key] = value.strip()
    return trailers


def validate_identity(payload: dict[str, Any]) -> dict[str, Any]:
    required = {
        "merge_commit_sha",
        "identity_sha",
        "source_sha",
        "version",
        "baseline_version",
        "version_input",
        "channel",
        "release_tag",
    }
    missing = sorted(required - set(payload))
    if missing:
        raise PolicyError(f"release identity missing: {', '.join(missing)}")
    for key in ("merge_commit_sha", "identity_sha", "source_sha"):
        validate_sha(str(payload[key]), key)
    result = compute_target(str(payload["baseline_version"]), str(payload["version_input"]))
    if result["version"] != payload["version"] or result["channel"] != payload["channel"]:
        raise PolicyError("release identity does not match its frozen version decision")
    if payload["release_tag"] != f"v{payload['version']}":
        raise PolicyError("release tag must be derived from VERSION only")
    return payload


def validate_failure_context(
    payload: dict[str, Any], *, expected_repository: str | None = None,
    expected_run_id: str | None = None, expected_server: str | None = None,
    expected_attempt: str | None = None,
) -> dict[str, Any]:
    if payload.get("identity_resolution_failed") is True:
        merge_sha = str(payload.get("merge_commit_sha", ""))
        if merge_sha:
            validate_sha(merge_sha, "merge_commit_sha")
        elif payload.get("identity_failure_kind") != "resolver-error":
            raise PolicyError("unresolved identity failure must not omit its merge SHA")
        if payload.get("identity_failure_kind") not in {"resolver-error", "no-identity"}:
            raise PolicyError("failure context identity failure kind is invalid")
        if payload.get("recovery_instruction") != (
            "verify the merged VERSION identity; retry Release with the same merge SHA only after identity is confirmed"
        ):
            raise PolicyError("unresolved identity recovery instruction is invalid")
        run_url = str(payload.get("run_url", ""))
        parsed = urllib.parse.urlparse(run_url)
        if parsed.scheme != "https" or not parsed.netloc:
            raise PolicyError("failure context run_url must be an HTTPS URL")
        if expected_repository and payload.get("repository") != expected_repository:
            raise PolicyError("failure context repository does not match the expected repository")
        if expected_server and (parsed.scheme + "://" + parsed.netloc) != expected_server.rstrip("/"):
            raise PolicyError("failure context run_url server does not match the expected server")
        if expected_run_id and not parsed.path.endswith(f"/actions/runs/{expected_run_id}"):
            raise PolicyError("failure context run_url does not match the expected run")
        if expected_attempt and str(payload.get("run_attempt")) != expected_attempt:
            raise PolicyError("failure context run_attempt does not match the expected attempt")
        return payload
    required = {
        "source_sha", "merge_commit_sha", "identity_sha", "version", "baseline_version",
        "version_input", "channel", "tag", "artifact_names", "run_url", "recovery_instruction",
    }
    missing = sorted(required - set(payload))
    if missing:
        raise PolicyError(f"failure context missing: {', '.join(missing)}")
    if not isinstance(payload["artifact_names"], list) or not payload["artifact_names"]:
        raise PolicyError("failure context artifact_names must be non-empty")
    artifact_digest = str(payload.get("artifact_digest", ""))
    if artifact_digest and not re.fullmatch(r"sha256:[0-9a-f]{64}", artifact_digest):
        raise PolicyError("failure context artifact_digest is not a SHA-256 digest")
    identity = {
        "merge_commit_sha": payload["merge_commit_sha"],
        "identity_sha": payload["identity_sha"],
        "source_sha": payload["source_sha"],
        "version": payload["version"],
        "baseline_version": payload["baseline_version"],
        "version_input": payload["version_input"],
        "channel": payload["channel"],
        "release_tag": payload["tag"],
    }
    validate_identity(identity)
    run_url = str(payload["run_url"])
    parsed = urllib.parse.urlparse(run_url)
    if parsed.scheme != "https" or not parsed.netloc:
        raise PolicyError("failure context run_url must be an HTTPS URL")
    if expected_repository and payload.get("repository") != expected_repository:
        raise PolicyError("failure context repository does not match the expected repository")
    if expected_server and (parsed.scheme + "://" + parsed.netloc) != expected_server.rstrip("/"):
        raise PolicyError("failure context run_url server does not match the expected server")
    if expected_run_id and not parsed.path.endswith(f"/actions/runs/{expected_run_id}"):
        raise PolicyError("failure context run_url does not match the expected run")
    if expected_attempt and str(payload.get("run_attempt")) != expected_attempt:
        raise PolicyError("failure context run_attempt does not match the expected attempt")
    if payload.get("recovery_instruction") != f"workflow_dispatch merge_sha={payload['merge_commit_sha']} recovery_reason=<required>":
        raise PolicyError("failure context recovery instruction must retry the immutable merge identity")
    return payload


def _cli() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    channel_parser = subparsers.add_parser("channel-for-version")
    channel_parser.add_argument("--version", required=True)
    validate_parser = subparsers.add_parser("validate-channel")
    validate_parser.add_argument("--version", required=True)
    validate_parser.add_argument("--channel", required=True)
    target_parser = subparsers.add_parser("compute-target")
    target_parser.add_argument("--baseline", required=True)
    target_parser.add_argument("--version", required=True)
    return parser


def main() -> int:
    args = _cli().parse_args()
    try:
        load_policy()
        if args.command == "channel-for-version":
            print(channel_for_version(args.version))
        elif args.command == "validate-channel":
            validate_channel_version(args.version, args.channel)
            print("valid")
        else:
            print(json.dumps(compute_target(args.baseline, args.version), sort_keys=True))
    except PolicyError as error:
        print(str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
