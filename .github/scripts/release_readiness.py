#!/usr/bin/env python3
"""Manage immutable exact-SHA release readiness receipts."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

import release_snapshot


SCHEMA_VERSION = 1
DEFAULT_NOTES_REF = "refs/notes/release-readiness"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
DIGEST_RE = re.compile(r"^[0-9a-f]{64}$")
ARTIFACT_RE = re.compile(r"^[a-z0-9][a-z0-9._-]*$")


class ReadinessError(RuntimeError):
    pass


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(["git", *args], text=True, capture_output=True, check=False)
    if check and result.returncode != 0:
        raise ReadinessError(result.stderr.strip() or result.stdout.strip() or "git command failed")
    return result


def git_output(*args: str) -> str:
    return git(*args).stdout.strip()


def validate_sha(value: Any, field: str) -> str:
    if not isinstance(value, str) or not SHA_RE.fullmatch(value):
        raise ReadinessError(f"{field} must be a 40-character lowercase commit SHA")
    return value


def validate_run_id(value: Any, field: str) -> int:
    if not isinstance(value, int) or value <= 0:
        raise ReadinessError(f"{field} must be a positive Actions run id")
    return value


def validate_digest(value: Any, field: str) -> str:
    if not isinstance(value, str) or not DIGEST_RE.fullmatch(value):
        raise ReadinessError(f"{field} must be a sha256 digest")
    return value


def validate_receipt(payload: Any, *, expected_sha: str | None = None) -> dict[str, Any]:
    if not isinstance(payload, dict):
        raise ReadinessError("readiness receipt must be an object")
    if payload.get("schema_version") != SCHEMA_VERSION:
        raise ReadinessError("unsupported readiness receipt schema")
    target_sha = validate_sha(payload.get("target_sha"), "target_sha")
    if expected_sha and target_sha != expected_sha:
        raise ReadinessError(f"readiness target_sha mismatch: expected {expected_sha}, got {target_sha}")
    if payload.get("candidate_workflow") != "Release Candidate Pipeline":
        raise ReadinessError("readiness candidate workflow is not trusted")
    if payload.get("candidate_event") != "push":
        raise ReadinessError("readiness candidate event must be push")
    if payload.get("verification_mode") is not False:
        raise ReadinessError("verification-mode runs must not write readiness receipts")
    if payload.get("publish") is not False:
        raise ReadinessError("readiness receipt publish marker must be false")
    validate_run_id(payload.get("candidate_run_id"), "candidate_run_id")
    if not isinstance(payload.get("created_at"), str) or not payload["created_at"]:
        raise ReadinessError("readiness created_at is required")

    source = payload.get("source_gate")
    if not isinstance(source, dict):
        raise ReadinessError("source_gate proof is required")
    if validate_sha(source.get("target_sha"), "source_gate.target_sha") != target_sha:
        raise ReadinessError("source gate target_sha does not match receipt")
    validate_run_id(source.get("run_id"), "source_gate.run_id")
    artifact_name = source.get("artifact_name")
    if not isinstance(artifact_name, str) or not ARTIFACT_RE.fullmatch(artifact_name):
        raise ReadinessError("source_gate.artifact_name is invalid")
    if artifact_name != f"source-gate-attestation-{source['run_id']}":
        raise ReadinessError("source gate artifact is not bound to its run id")
    if source.get("gate_result") != "success":
        raise ReadinessError("source gate result must be success")
    validate_digest(source.get("attestation_sha256"), "source_gate.attestation_sha256")
    if source.get("publish") is not False:
        raise ReadinessError("source gate publish marker must be false")

    preparation = payload.get("preparation")
    if not isinstance(preparation, dict):
        raise ReadinessError("preparation proof is required")
    if validate_sha(preparation.get("target_sha"), "preparation.target_sha") != target_sha:
        raise ReadinessError("preparation target_sha does not match receipt")
    validate_run_id(preparation.get("run_id"), "preparation.run_id")
    artifact_name = preparation.get("artifact_name")
    if not isinstance(artifact_name, str) or not artifact_name.startswith("release-preparation-"):
        raise ReadinessError("preparation.artifact_name is invalid")
    if artifact_name != f"release-preparation-{target_sha}":
        raise ReadinessError("preparation artifact is not bound to target_sha")
    validate_digest(preparation.get("manifest_sha256"), "preparation.manifest_sha256")
    if preparation.get("publish") is not False:
        raise ReadinessError("preparation publish marker must be false")
    return payload


def read_receipt(notes_ref: str, target_sha: str) -> dict[str, Any] | None:
    result = git("notes", f"--ref={notes_ref}", "show", target_sha, check=False)
    if result.returncode != 0:
        return None
    try:
        return validate_receipt(json.loads(result.stdout), expected_sha=target_sha)
    except json.JSONDecodeError as error:
        raise ReadinessError(f"readiness note for {target_sha} is not JSON") from error


def fetch_notes(notes_ref: str) -> None:
    if git("remote", "get-url", "origin", check=False).returncode != 0:
        return
    if git("ls-remote", "--exit-code", "origin", notes_ref, check=False).returncode == 0:
        git("fetch", "--no-tags", "origin", f"+{notes_ref}:{notes_ref}")


def write_receipt(payload: dict[str, Any], notes_ref: str, max_attempts: int) -> None:
    target_sha = validate_sha(payload.get("target_sha"), "target_sha")
    validate_receipt(payload, expected_sha=target_sha)
    for attempt in range(1, max_attempts + 1):
        fetch_notes(notes_ref)
        existing = read_receipt(notes_ref, target_sha)
        if existing is not None:
            if existing != payload:
                raise ReadinessError(f"immutable readiness receipt already exists for {target_sha}")
            return
        with tempfile.TemporaryDirectory(prefix="release-readiness-") as directory:
            path = Path(directory) / "receipt.json"
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
            git("notes", f"--ref={notes_ref}", "add", "-f", "-F", str(path), target_sha)
        push = git("push", "origin", notes_ref, check=False)
        if push.returncode == 0:
            return
        if attempt == max_attempts:
            raise ReadinessError(push.stderr.strip() or "failed to publish readiness notes")
        time.sleep(1)
    raise ReadinessError("readiness receipt retry loop exhausted")


def export_values(values: dict[str, Any], output: str) -> None:
    lines = []
    for key, value in values.items():
        if isinstance(value, bool):
            rendered = "true" if value else "false"
        elif value is None:
            rendered = ""
        else:
            rendered = str(value)
        lines.append(f"{key}={rendered}")
    text = "\n".join(lines) + "\n"
    if output:
        with Path(output).open("a", encoding="utf-8") as handle:
            handle.write(text)
    else:
        sys.stdout.write(text)


def export_receipt(receipt: dict[str, Any], output: str) -> None:
    export_values(
        {
            "readiness_target_sha": receipt["target_sha"],
            "candidate_run_id": receipt["candidate_run_id"],
            "source_gate_run_id": receipt["source_gate"]["run_id"],
            "source_gate_artifact_name": receipt["source_gate"]["artifact_name"],
            "source_gate_attestation_sha256": receipt["source_gate"]["attestation_sha256"],
            "preparation_run_id": receipt["preparation"]["run_id"],
            "preparation_artifact_name": receipt["preparation"]["artifact_name"],
            "preparation_manifest_sha256": receipt["preparation"]["manifest_sha256"],
            "readiness_publish": receipt["publish"],
        },
        output,
    )


def pending_ready_targets(args: argparse.Namespace) -> list[str]:
    upper_bound = args.upper_bound or git_output("rev-parse", args.main_ref)
    validate_sha(upper_bound, "upper_bound")
    git("merge-base", "--is-ancestor", upper_bound, args.main_ref)
    fetch_notes(args.readiness_notes_ref)
    release_snapshot.fetch_notes_ref(args.snapshot_notes_ref)
    release_snapshot.fetch_notes_ref(args.publication_notes_ref)
    release_snapshot.fetch_notes_ref(args.override_notes_ref)
    release_snapshot.fetch_tags()
    ready: list[str] = []
    for target_sha in release_snapshot.pending_release_targets(
        args.snapshot_notes_ref,
        upper_bound,
        publication_notes_ref=args.publication_notes_ref,
        override_notes_ref=args.override_notes_ref,
    ):
        if read_receipt(args.readiness_notes_ref, target_sha) is not None:
            ready.append(target_sha)
    return ready


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Manage release readiness receipts")
    sub = parser.add_subparsers(dest="command", required=True)
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--readiness-notes-ref", default=DEFAULT_NOTES_REF)
    common.add_argument("--snapshot-notes-ref", default=release_snapshot.DEFAULT_NOTES_REF)
    common.add_argument("--publication-notes-ref", default=release_snapshot.DEFAULT_PUBLICATION_NOTES_REF)
    common.add_argument("--override-notes-ref", default=release_snapshot.DEFAULT_OVERRIDE_NOTES_REF)
    common.add_argument("--main-ref", default="origin/main")
    common.add_argument("--upper-bound", default="")
    common.add_argument("--github-output", default="")
    next_ready = sub.add_parser("next-ready", parents=[common])
    require = sub.add_parser("require", parents=[common])
    require.add_argument("--target-sha", required=True)
    export = sub.add_parser("export", parents=[])
    export.add_argument("--target-sha", required=True)
    export.add_argument("--readiness-notes-ref", default=DEFAULT_NOTES_REF)
    export.add_argument("--github-output", default="")
    write = sub.add_parser("write")
    write.add_argument("--receipt", type=Path, required=True)
    write.add_argument("--readiness-notes-ref", default=DEFAULT_NOTES_REF)
    write.add_argument("--max-attempts", type=int, default=3)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.command == "write":
            payload = json.loads(args.receipt.read_text())
            write_receipt(payload, args.readiness_notes_ref, args.max_attempts)
            return 0
        if args.command == "export":
            fetch_notes(args.readiness_notes_ref)
            receipt = read_receipt(args.readiness_notes_ref, validate_sha(args.target_sha, "target_sha"))
            if receipt is None:
                raise ReadinessError(f"missing readiness receipt for {args.target_sha}")
            export_receipt(receipt, args.github_output)
            return 0
        if args.command == "require":
            target_sha = validate_sha(args.target_sha, "target_sha")
            if target_sha not in pending_ready_targets(args):
                raise ReadinessError(f"target {target_sha} is not ready or is not pending")
            export_values({"target_sha": target_sha}, args.github_output)
            return 0
        ready = pending_ready_targets(args)
        export_values({"target_sha": ready[0] if ready else ""}, args.github_output)
        return 0
    except (ReadinessError, OSError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
