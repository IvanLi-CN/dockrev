#!/usr/bin/env python3
"""Manage immutable exact-SHA release readiness receipts and FIFO recovery."""

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


def _validate_receipt_entry(payload: Any, *, expected_sha: str | None = None) -> dict[str, Any]:
    if not isinstance(payload, dict):
        raise ReadinessError("readiness receipt must be an object")
    if payload.get("schema_version") != SCHEMA_VERSION:
        raise ReadinessError("unsupported readiness receipt schema")
    target_sha = validate_sha(payload.get("target_sha"), "target_sha")
    if expected_sha and target_sha != expected_sha:
        raise ReadinessError(f"readiness target_sha mismatch: expected {expected_sha}, got {target_sha}")
    if payload.get("candidate_workflow") != "Release Candidate Pipeline":
        raise ReadinessError("readiness candidate workflow is not trusted")
    operation = payload.get("operation", "push")
    if not isinstance(operation, str):
        raise ReadinessError("readiness operation must be a string")
    if operation not in {"push", "recover"}:
        raise ReadinessError("readiness operation must be push or recover")
    candidate_event = payload.get("candidate_event")
    if operation == "push" and candidate_event != "push":
        raise ReadinessError("push readiness candidate event must be push")
    if operation == "recover" and candidate_event != "workflow_dispatch":
        raise ReadinessError("recovery readiness candidate event must be workflow_dispatch")
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
    if operation == "recover":
        recovery = payload.get("recovery")
        if not isinstance(recovery, dict):
            raise ReadinessError("recovery audit proof is required")
        if recovery.get("mode") != "candidate-recovery":
            raise ReadinessError("recovery audit mode must be candidate-recovery")
        if validate_sha(recovery.get("target_sha"), "recovery.target_sha") != target_sha:
            raise ReadinessError("recovery target_sha does not match receipt")
        actor = recovery.get("actor")
        if not isinstance(actor, str) or not actor.strip():
            raise ReadinessError("recovery actor is required")
        reason = recovery.get("reason")
        if not isinstance(reason, str) or not reason.strip():
            raise ReadinessError("recovery reason is required")
    return payload


def validate_receipt(payload: Any, *, expected_sha: str | None = None) -> dict[str, Any]:
    if isinstance(payload, dict) and "receipt_ledger" in payload:
        if payload.get("schema_version") != SCHEMA_VERSION:
            raise ReadinessError("unsupported readiness receipt schema")
        target_sha = validate_sha(payload.get("target_sha"), "target_sha")
        if expected_sha and target_sha != expected_sha:
            raise ReadinessError(f"readiness target_sha mismatch: expected {expected_sha}, got {target_sha}")
        entries = payload.get("receipt_ledger")
        if not isinstance(entries, list) or not entries:
            raise ReadinessError("readiness receipt ledger must contain entries")
        validated = [_validate_receipt_entry(entry, expected_sha=target_sha) for entry in entries]
        return validated[-1]
    return _validate_receipt_entry(payload, expected_sha=expected_sha)


def read_receipt_ledger(notes_ref: str, target_sha: str) -> list[dict[str, Any]]:
    result = git("notes", f"--ref={notes_ref}", "show", target_sha, check=False)
    if result.returncode != 0:
        return []
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ReadinessError(f"readiness note for {target_sha} is not JSON") from error
    if isinstance(payload, dict) and "receipt_ledger" in payload:
        if payload.get("schema_version") != SCHEMA_VERSION:
            raise ReadinessError("unsupported readiness receipt schema")
        if validate_sha(payload.get("target_sha"), "target_sha") != target_sha:
            raise ReadinessError("readiness ledger target_sha does not match note target")
        entries = payload.get("receipt_ledger")
        if not isinstance(entries, list) or not entries:
            raise ReadinessError("readiness receipt ledger must contain entries")
        return [_validate_receipt_entry(entry, expected_sha=target_sha) for entry in entries]
    return [_validate_receipt_entry(payload, expected_sha=target_sha)]


def read_receipt(notes_ref: str, target_sha: str) -> dict[str, Any] | None:
    entries = read_receipt_ledger(notes_ref, target_sha)
    if not entries:
        return None
    return entries[-1]


def fetch_notes(notes_ref: str) -> None:
    if git("remote", "get-url", "origin", check=False).returncode != 0:
        return
    if git("ls-remote", "--exit-code", "origin", notes_ref, check=False).returncode == 0:
        git("fetch", "--no-tags", "origin", f"+{notes_ref}:{notes_ref}")


def write_receipt(payload: dict[str, Any], notes_ref: str, max_attempts: int) -> None:
    target_sha = validate_sha(payload.get("target_sha"), "target_sha")
    payload = _validate_receipt_entry(payload, expected_sha=target_sha)
    for attempt in range(1, max_attempts + 1):
        fetch_notes(notes_ref)
        existing_entries = read_receipt_ledger(notes_ref, target_sha)
        if payload in existing_entries:
            return
        ledger = {
            "schema_version": SCHEMA_VERSION,
            "target_sha": target_sha,
            "receipt_ledger": [*existing_entries, payload],
        }
        with tempfile.TemporaryDirectory(prefix="release-readiness-") as directory:
            path = Path(directory) / "receipt.json"
            path.write_text(json.dumps(ledger, indent=2, sort_keys=True) + "\n")
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
    missing_release_resolver = None
    if getattr(args, "github_repository", "") and getattr(args, "github_token", ""):
        missing_release_resolver = lambda commit: release_snapshot.release_enabled_for_commit(
            args.api_root,
            args.github_repository,
            args.github_token,
            commit,
        )
    pending = release_snapshot.pending_release_targets(
        args.snapshot_notes_ref,
        upper_bound,
        publication_notes_ref=args.publication_notes_ref,
        override_notes_ref=args.override_notes_ref,
        strict_fifo=True,
        release_enabled_for_missing=missing_release_resolver,
    )
    if not pending:
        return []
    first_pending = pending[0]
    return [first_pending] if read_receipt(args.readiness_notes_ref, first_pending) is not None else []


def recover_preflight(args: argparse.Namespace) -> int:
    target_sha = validate_sha(args.target_sha, "target_sha")
    git("merge-base", "--is-ancestor", target_sha, args.main_ref)
    release_snapshot.fetch_notes_ref(args.publication_notes_ref)
    release_snapshot.fetch_notes_ref(args.override_notes_ref)
    release_snapshot.fetch_tags()
    tagged = release_snapshot.released_commits_from_tags(target_sha)
    main_commits = release_snapshot.first_parent_commits(args.main_ref)
    if target_sha not in main_commits:
        raise ReadinessError(f"recovery target {target_sha} is not on the main first-parent chain")
    target_index = main_commits.index(target_sha)
    if target_sha in tagged:
        raise ReadinessError(f"recovery target {target_sha} already has a release tag without complete ledger")
    tagged_indices = [main_commits.index(commit) for commit in tagged if commit in main_commits]
    anchor_index = max(tagged_indices, default=-1)
    released = {
        commit
        for commit in main_commits[anchor_index + 1 : target_index + 1]
        if release_snapshot.read_publication(args.publication_notes_ref, commit) is not None
    }
    skipped: set[str] = set()
    for commit in main_commits[anchor_index + 1 : target_index + 1]:
        override = release_snapshot.read_override(args.override_notes_ref, commit)
        if override is not None and override.get("status") == "skip":
            skipped.add(commit)
    pending: list[str] = []
    for commit in main_commits[anchor_index + 1 : target_index + 1]:
        if commit in skipped:
            continue
        if commit in tagged and commit not in released:
            raise ReadinessError(f"recovery target prefix contains tag-only publication without complete ledger: {commit}")
        pr = release_snapshot.load_pr_for_commit(
            args.api_root,
            args.repository,
            args.token,
            commit,
            allow_zero=True,
        )
        if pr is None:
            continue
        type_label, _channel_label = release_snapshot.parse_release_labels(release_snapshot.current_pr_labels(pr))
        if type_label in {"type:docs", "type:skip"} or commit in released:
            continue
        pending.append(commit)
    if not pending:
        raise ReadinessError(f"target {target_sha} is not a release-enabled unreleased first-parent commit")
    if pending[0] != target_sha:
        raise ReadinessError(f"recovery target {target_sha} is not the oldest unreleased target {pending[0]}")
    export_values({"target_sha": target_sha, "release_enabled": True}, args.github_output)
    return 0


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
    common.add_argument("--github-repository", default="")
    common.add_argument("--github-token", default="")
    common.add_argument("--api-root", default="https://api.github.com")
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
    preflight = sub.add_parser("recover-preflight")
    preflight.add_argument("--target-sha", required=True)
    preflight.add_argument("--repository", required=True)
    preflight.add_argument("--token", required=True)
    preflight.add_argument("--api-root", default="https://api.github.com")
    preflight.add_argument("--main-ref", default="origin/main")
    preflight.add_argument("--publication-notes-ref", default=release_snapshot.DEFAULT_PUBLICATION_NOTES_REF)
    preflight.add_argument("--override-notes-ref", default=release_snapshot.DEFAULT_OVERRIDE_NOTES_REF)
    preflight.add_argument("--github-output", default="")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.command == "write":
            payload = json.loads(args.receipt.read_text())
            write_receipt(payload, args.readiness_notes_ref, args.max_attempts)
            return 0
        if args.command == "recover-preflight":
            return recover_preflight(args)
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
