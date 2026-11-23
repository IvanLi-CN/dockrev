#!/usr/bin/env python3
"""Manual version allocation from a trusted final release baseline."""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.parse
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[2]
POLICY_PATH = ROOT / ".github/manual-version-release.json"
VERSION_RE = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-(alpha|beta|rc)\.(0|[1-9][0-9]*))?$"
)
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
IMAGE_DIGEST_NAMES = {"dockrev", "dockrev-supervisor"}
IMAGE_DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
PREFLIGHT_RECOVERY_INSTRUCTION = (
    "fix the preflight failure, then re-run Release from main with the same target"
)
CHANNEL_RANK = {"alpha": 0, "beta": 1, "rc": 2}


class PolicyError(ValueError):
    pass


def load_policy(path: Path = POLICY_PATH) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PolicyError(f"manual version release policy is unavailable: {error}") from error
    expected = {
        "schema_version": 2,
        "baseline": {"source": "github-final-release", "base_branch": "main"},
        "source_ci_workflow": "CI (main)",
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
            "stable:alpha,beta,rc",
            "alpha:alpha,beta",
            "beta:beta,rc",
            "rc:rc,stable",
        ],
        "publication": {
            "stable": {
                "github_release": True,
                "ghcr_version": True,
                "ghcr_source_sha": True,
                "ghcr_latest": True,
            },
            "prerelease": {
                "github_prerelease": True,
                "ghcr_version": True,
                "ghcr_source_sha": True,
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


def validate_image_digests(value: Any) -> dict[str, str]:
    if value in (None, {}):
        return {}
    if not isinstance(value, dict):
        raise PolicyError("image digests must be a mapping of image name to sha256 digest")
    if set(value) != IMAGE_DIGEST_NAMES:
        raise PolicyError("image digests must cover dockrev and dockrev-supervisor")
    for name, digest in value.items():
        if not isinstance(digest, str) or not IMAGE_DIGEST_RE.fullmatch(digest):
            raise PolicyError(f"image digest for {name} must be a sha256 digest")
    return value


def next_core(version: str, intent: str) -> tuple[int, int, int]:
    major, minor, patch, _channel, _sequence = parse_version(version)
    if intent == "major":
        return major + 1, 0, 0
    if intent == "minor":
        return major, minor + 1, 0
    if intent == "patch":
        return major, minor, patch + 1
    raise PolicyError(f"unsupported numeric version intent: {intent}")


def _prerelease_state(tags: Iterable[str]) -> dict[tuple[int, int, int], dict[str, int]]:
    state: dict[tuple[int, int, int], dict[str, int]] = {}
    for tag in tags:
        version = tag[1:] if tag.startswith("v") else tag
        try:
            major, minor, patch, channel, sequence = parse_version(version)
        except PolicyError:
            continue
        if channel is None or sequence is None:
            continue
        channels = state.setdefault((major, minor, patch), {})
        channels[channel] = max(channels.get(channel, 0), sequence)
    return state


def _next_prerelease_version(
    baseline: str, channel: str, tags: Iterable[str],
) -> str:
    major, minor, patch, _current, _sequence = parse_version(baseline)
    next_patch_core = (major, minor, patch + 1)
    state = _prerelease_state(tags)
    higher_cores = [core for core in state if core > next_patch_core]
    if higher_cores:
        core = max(higher_cores)
    else:
        core = next_patch_core
    channels = state.get(core, {})
    if not channels:
        return f"{core[0]}.{core[1]}.{core[2]}-{channel}.1"
    highest_channel = max(channels, key=lambda name: CHANNEL_RANK[name])
    highest_rank = CHANNEL_RANK[highest_channel]
    target_rank = CHANNEL_RANK[channel]
    if target_rank < highest_rank:
        raise PolicyError(
            f"cannot move from {highest_channel} back to {channel} on {core[0]}.{core[1]}.{core[2]}"
        )
    if target_rank == highest_rank:
        sequence = channels[highest_channel] + 1
    elif target_rank == highest_rank + 1:
        sequence = 1
    else:
        raise PolicyError(
            f"cannot skip from {highest_channel} to {channel} on {core[0]}.{core[1]}.{core[2]}"
        )
    return f"{core[0]}.{core[1]}.{core[2]}-{channel}.{sequence}"


def compute_target(
    baseline: str, version_input: str, prerelease_tags: Iterable[str] = (),
) -> dict[str, str]:
    major, minor, patch, baseline_channel, _sequence = parse_version(baseline)
    if baseline_channel is not None:
        raise PolicyError("release baseline must be a final version")
    if not version_input or version_input != version_input.strip():
        raise PolicyError("version input must be a non-empty canonical value")
    tags = list(prerelease_tags)
    baseline_version = f"{major}.{minor}.{patch}"
    if version_input in {"major", "minor", "patch"}:
        core = next_core(baseline_version, version_input)
        version = f"{core[0]}.{core[1]}.{core[2]}"
    elif version_input in {"alpha", "beta", "rc"}:
        version = _next_prerelease_version(baseline_version, version_input, tags)
    else:
        version = version_input
        target_channel = channel_for_version(version)
        core = parse_version(version)[:3]
        if target_channel == "stable":
            numeric_targets = {
                f"{parts[0]}.{parts[1]}.{parts[2]}"
                for intent_name in ("major", "minor", "patch")
                for parts in (next_core(baseline_version, intent_name),)
            }
            state = _prerelease_state(tags)
            promoted = any(
                candidate_core == core and "rc" in channels
                for candidate_core, channels in state.items()
                if candidate_core > (major, minor, patch)
            )
            if version not in numeric_targets and not promoted:
                raise PolicyError(
                    "exact stable version must match a calculated target or promote an existing rc"
                )
        else:
            expected = _next_prerelease_version(baseline_version, target_channel, tags)
            if version != expected:
                raise PolicyError(f"exact prerelease version must equal the next valid identity: {expected}")
    validate_channel_version(version, channel_for_version(version))
    return {
        "baseline_version": baseline_version,
        "version_input": version_input,
        "version": version,
        "channel": channel_for_version(version),
    }


def validate_failure_context(payload: dict[str, Any]) -> dict[str, Any]:
    validate_sha(str(payload.get("source_sha", "")), "source_sha")
    run_url = str(payload.get("run_url", ""))
    parsed = urllib.parse.urlparse(run_url)
    if parsed.scheme != "https" or not parsed.netloc:
        raise PolicyError("failure context run_url must be an HTTPS URL")
    target = payload.get("target")
    if not isinstance(target, str) or not target:
        raise PolicyError("failure context target is required")
    version = payload.get("version")
    if version in (None, ""):
        if payload.get("recovery_instruction") != PREFLIGHT_RECOVERY_INSTRUCTION:
            raise PolicyError("preflight failure recovery instruction is invalid")
        if payload.get("identity_failure_kind") != "preflight":
            raise PolicyError("failure context preflight kind is invalid")
        return payload
    if not isinstance(version, str):
        raise PolicyError("failure context version is invalid")
    channel = channel_for_version(version)
    if payload.get("channel") != channel:
        raise PolicyError("failure context channel does not match its version")
    baseline = str(payload.get("baseline_version", ""))
    _major, _minor, _patch, baseline_channel, _sequence = parse_version(baseline)
    if baseline_channel is not None:
        raise PolicyError("failure context baseline must be a final version")
    version_input = str(payload.get("version_input", ""))
    if version_input in {"major", "minor", "patch"}:
        if channel != "stable":
            raise PolicyError("numeric version intent must resolve to a stable version")
    elif version_input in {"alpha", "beta", "rc"}:
        if channel != version_input:
            raise PolicyError("prerelease intent must match the resolved channel")
    elif version_input != version:
        raise PolicyError("exact version input must match the resolved version")
    if payload.get("tag") != f"v{version}":
        raise PolicyError("failure context tag must be derived from the version")
    artifact_names = payload.get("artifact_names")
    if not isinstance(artifact_names, list) or not artifact_names:
        raise PolicyError("failure context artifact_names must be non-empty")
    artifact_digest = str(payload.get("artifact_digest", ""))
    if artifact_digest and not re.fullmatch(r"(?:sha256:)?[0-9a-f]{64}", artifact_digest):
        raise PolicyError("failure context artifact_digest is not a SHA-256 digest")
    validate_image_digests(payload.get("image_digests"))
    expected_recovery = f"re-run Release from main with target={payload.get('version_input')}"
    if payload.get("recovery_instruction") != expected_recovery:
        raise PolicyError("failure context recovery instruction must reuse the same target")
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
    target_parser.add_argument("--tags", type=Path, default=None)
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
            tags = args.tags.read_text(encoding="utf-8").split() if args.tags else []
            print(json.dumps(compute_target(args.baseline, args.version, tags), sort_keys=True))
    except (PolicyError, OSError) as error:
        print(str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
