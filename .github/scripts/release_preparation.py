#!/usr/bin/env python3
"""Build and validate exact-SHA release preparation manifests.

The preparation workflow is a reusable child of the release candidate
pipeline. It never waits for another Actions run and it never has publish
permissions; the candidate pipeline records its immutable artifact proof.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import release_snapshot


SCHEMA_VERSION = 2
WORKFLOW_FILE = "release-preparation.yml"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
DIGEST_RE = re.compile(r"^[0-9a-f]{64}$")
ALLOWED_EVENTS = {"workflow_call", "workflow_dispatch"}
FIXED_FILES = (
    "target/ci/amd64/release/dockrev",
    "target/ci/amd64/release/dockrev-supervisor",
    "target/ci/amd64/x86_64-unknown-linux-musl/release/dockrev",
    "target/ci/amd64/x86_64-unknown-linux-musl/release/dockrev-supervisor",
    "target/ci/arm64/release/dockrev",
    "target/ci/arm64/release/dockrev-supervisor",
    "target/ci/arm64/aarch64-unknown-linux-musl/release/dockrev",
    "target/ci/arm64/aarch64-unknown-linux-musl/release/dockrev-supervisor",
    "dist/ci/docker/amd64/dockrev",
    "dist/ci/docker/amd64/dockrev-supervisor",
    "dist/ci/docker/arm64/dockrev",
    "dist/ci/docker/arm64/dockrev-supervisor",
)
REQUIRED_WEB_FILES = ("web/dist/.dockrev-route-contract.json",)


class PreparationError(RuntimeError):
    pass


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def expected_files(root: Path) -> list[str]:
    web_root = root / "web/dist"
    web_files = []
    if web_root.is_dir():
        web_files = [
            path.relative_to(root).as_posix()
            for path in web_root.rglob("*")
            if path.is_file() and not path.is_symlink()
        ]
    return sorted(set(FIXED_FILES) | set(REQUIRED_WEB_FILES) | set(web_files))


def build_manifest(
    root: Path,
    *,
    target_sha: str,
    event: str,
    workflow_sha: str,
    workflow_ref: str,
    verification_mode: bool = False,
) -> dict[str, Any]:
    if not SHA_RE.fullmatch(target_sha):
        raise PreparationError("target_sha must be a 40-character lowercase commit SHA")
    if event not in ALLOWED_EVENTS:
        raise PreparationError("preparation event is not allowed")
    if event == "workflow_dispatch" and not verification_mode:
        raise PreparationError("workflow_dispatch preparation requires verification_mode=true")

    files = []
    for relative in expected_files(root):
        path = root / relative
        if not path.is_file() or path.is_symlink():
            raise PreparationError(f"missing or unsafe preparation file: {relative}")
        files.append({"path": relative, "size": path.stat().st_size, "sha256": sha256_file(path)})

    return {
        "schema_version": SCHEMA_VERSION,
        "target_sha": target_sha,
        "event": event,
        "head_branch": "main",
        "workflow_file": WORKFLOW_FILE,
        "workflow_sha": workflow_sha,
        "workflow_ref": workflow_ref,
        "verification_mode": verification_mode,
        "publish": False,
        "created_at": utc_now(),
        "files": files,
    }


def validate_manifest(payload: Any, target_sha: str, *, allow_verification: bool = False) -> tuple[bool, str]:
    if not isinstance(payload, dict):
        return False, "preparation manifest must be an object"
    if payload.get("schema_version") != SCHEMA_VERSION:
        return False, "preparation manifest schema is unsupported"
    if payload.get("target_sha") != target_sha:
        return False, "preparation manifest target_sha does not match"
    if payload.get("head_branch") != "main" or payload.get("workflow_file") != WORKFLOW_FILE:
        return False, "preparation manifest source is not trusted main workflow"
    if not SHA_RE.fullmatch(str(payload.get("workflow_sha", ""))):
        return False, "preparation manifest workflow_sha is invalid"
    if not str(payload.get("workflow_ref", "")).endswith("@refs/heads/main"):
        return False, "preparation manifest workflow_ref is not trusted main"
    if payload.get("publish") is not False:
        return False, "preparation manifest publish marker must be false"
    event = payload.get("event")
    verification_mode = payload.get("verification_mode")
    if event not in ALLOWED_EVENTS:
        return False, "preparation manifest event is invalid"
    if not isinstance(verification_mode, bool):
        return False, "preparation manifest verification_mode must be boolean"
    if event == "workflow_dispatch" and not verification_mode:
        return False, "manual preparation must be verification-only"
    if verification_mode and not allow_verification:
        return False, "verification-only manifest cannot be release evidence"

    files = payload.get("files")
    if not isinstance(files, list) or not files:
        return False, "preparation manifest file list is empty"
    paths: set[str] = set()
    for entry in files:
        if not isinstance(entry, dict) or set(entry) != {"path", "size", "sha256"}:
            return False, "preparation manifest file entry is malformed"
        path = entry.get("path")
        if not isinstance(path, str) or not path or path in paths or path.startswith("/") or ".." in Path(path).parts:
            return False, "preparation manifest contains an unsafe or duplicate path"
        if not isinstance(entry.get("size"), int) or entry["size"] < 0:
            return False, "preparation manifest file size is invalid"
        if not re.fullmatch(r"[0-9a-f]{64}", str(entry.get("sha256"))):
            return False, "preparation manifest file digest is invalid"
        paths.add(path)
    if not set(FIXED_FILES).issubset(paths):
        return False, "preparation manifest is missing a required release file"
    if not any(path.startswith("web/dist/") for path in paths):
        return False, "preparation manifest is missing web/dist files"
    if not set(REQUIRED_WEB_FILES).issubset(paths):
        return False, "preparation manifest is missing the web route contract"
    return True, "ok"


def verify_manifest_files(
    root: Path,
    payload: Any,
    target_sha: str,
    *,
    expected_manifest_sha256: str | None = None,
    allow_verification: bool = False,
) -> tuple[bool, str]:
    valid, reason = validate_manifest(payload, target_sha, allow_verification=allow_verification)
    if not valid:
        return False, reason
    if expected_manifest_sha256 is not None:
        if not DIGEST_RE.fullmatch(expected_manifest_sha256):
            return False, "expected preparation manifest digest is invalid"
        if manifest_digest(payload) != expected_manifest_sha256:
            return False, "preparation manifest digest does not match expected"

    root = root.resolve()
    for entry in payload["files"]:
        relative = entry["path"]
        path = root / relative
        try:
            path.resolve(strict=False).relative_to(root)
        except ValueError:
            return False, f"preparation file is missing or unsafe: {relative}"
        if path.is_symlink() or not path.is_file():
            return False, f"preparation file is missing or unsafe: {relative}"
        if path.stat().st_size != entry["size"]:
            return False, f"preparation file size does not match: {relative}"
        if sha256_file(path) != entry["sha256"]:
            return False, f"preparation file digest does not match: {relative}"
    return True, "ok"


def manifest_digest(payload: dict[str, Any]) -> str:
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    candidate = sub.add_parser("candidate")
    candidate.add_argument("--target-sha", required=True)
    candidate.add_argument("--repository", required=True)
    candidate.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))
    candidate.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    candidate.add_argument("--output", required=True)
    manifest = sub.add_parser("manifest")
    manifest.add_argument("--root", default=".")
    manifest.add_argument("--target-sha", required=True)
    manifest.add_argument("--event", required=True)
    manifest.add_argument("--workflow-sha", required=True)
    manifest.add_argument("--workflow-ref", required=True)
    manifest.add_argument("--verification-mode", action="store_true")
    validate = sub.add_parser("validate")
    validate.add_argument("--manifest", type=Path, required=True)
    validate.add_argument("--target-sha", required=True)
    validate.add_argument("--allow-verification", action="store_true")
    verify = sub.add_parser("verify")
    verify.add_argument("--root", default=".")
    verify.add_argument("--manifest", type=Path, required=True)
    verify.add_argument("--target-sha", required=True)
    verify.add_argument("--expected-manifest-sha256")
    verify.add_argument("--allow-verification", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.command == "candidate":
            pr = release_snapshot.load_pr_for_commit(args.api_root, args.repository, args.token, args.target_sha)
            type_label, _channel_label = release_snapshot.parse_release_labels(release_snapshot.current_pr_labels(pr))
            payload = {"target_sha": args.target_sha, "release_enabled": type_label not in {"type:docs", "type:skip"}}
            Path(args.output).write_text(json.dumps(payload, sort_keys=True) + "\n")
            return 0
        if args.command == "manifest":
            payload = build_manifest(
                Path(args.root),
                target_sha=args.target_sha,
                event=args.event,
                workflow_sha=args.workflow_sha,
                workflow_ref=args.workflow_ref,
                verification_mode=args.verification_mode,
            )
            Path(args.output).write_text(json.dumps(payload, sort_keys=True) + "\n")
            return 0
        payload = json.loads(args.manifest.read_text())
        if args.command == "validate":
            valid, reason = validate_manifest(payload, args.target_sha, allow_verification=args.allow_verification)
        else:
            valid, reason = verify_manifest_files(
                Path(args.root),
                payload,
                args.target_sha,
                expected_manifest_sha256=args.expected_manifest_sha256,
                allow_verification=args.allow_verification,
            )
        print(reason)
        return 0 if valid else 1
    except (PreparationError, OSError, json.JSONDecodeError) as error:
        print(str(error), file=os.sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
