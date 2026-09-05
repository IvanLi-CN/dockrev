#!/usr/bin/env python3
"""Prepare and validate exact-SHA release artifacts without publication rights."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import time
import urllib.parse
import urllib.request
import zipfile
from datetime import datetime, timezone
from io import BytesIO
from pathlib import Path
from typing import Any

import release_snapshot


SCHEMA_VERSION = 1
WORKFLOW_FILE = "release-preparation.yml"
ARTIFACT_PREFIX = "release-preparation-"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
DIGEST_RE = re.compile(r"^[0-9a-f]{64}$")
RECOVERY_RE = re.compile(r"^release-recovery-[0-9a-f]{40}$")
# Before this commit, upload-artifact omitted the required hidden route
# contract. A failed recovery from that workflow cannot count against the
# current recovery allowance.
ARTIFACT_UPLOAD_CONTRACT_SHA = "46cb8a59d2d6196136985999f2fea153cd904686"
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
REQUIRED_WEB_FILES = (
    "web/dist/.dockrev-route-contract.json",
)


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
    recovery_request: str,
) -> dict[str, Any]:
    if not SHA_RE.fullmatch(target_sha):
        raise PreparationError("target_sha must be a 40-character lowercase commit SHA")
    if event not in {"push", "workflow_dispatch"}:
        raise PreparationError("preparation event is not allowed")
    if event == "workflow_dispatch" and not RECOVERY_RE.fullmatch(recovery_request):
        raise PreparationError("recovery workflow_dispatch requires a target-bound recovery request")
    if event == "push" and recovery_request:
        raise PreparationError("push preparation must not carry a recovery request")

    files = []
    for relative in expected_files(root):
        path = root / relative
        if not path.is_file() or path.is_symlink():
            raise PreparationError(f"missing or unsafe preparation file: {relative}")
        files.append(
            {
                "path": relative,
                "size": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )

    return {
        "schema_version": SCHEMA_VERSION,
        "target_sha": target_sha,
        "event": event,
        "head_branch": "main",
        "workflow_file": WORKFLOW_FILE,
        "workflow_sha": workflow_sha,
        "workflow_ref": workflow_ref,
        "recovery_request": recovery_request,
        "publish": False,
        "created_at": utc_now(),
        "files": files,
    }


def validate_manifest(payload: Any, target_sha: str, *, allow_recovery: bool = True) -> tuple[bool, str]:
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
    recovery_request = payload.get("recovery_request", "")
    if event == "push":
        if recovery_request:
            return False, "push preparation must not carry a recovery request"
    elif event == "workflow_dispatch":
        if not allow_recovery or not RECOVERY_RE.fullmatch(str(recovery_request)):
            return False, "recovery preparation request is invalid"
        if recovery_request != f"release-recovery-{target_sha}":
            return False, "recovery preparation request does not match target_sha"
    else:
        return False, "preparation manifest event is invalid"

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
    allow_recovery: bool = True,
    expected_manifest_sha256: str | None = None,
) -> tuple[bool, str]:
    valid, reason = validate_manifest(payload, target_sha, allow_recovery=allow_recovery)
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


class RedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):  # type: ignore[no-untyped-def]
        redirected = super().redirect_request(req, fp, code, msg, headers, newurl)
        if redirected is None:
            return None
        source = urllib.parse.urlsplit(req.full_url)
        destination = urllib.parse.urlsplit(redirected.full_url)
        if (source.scheme, source.netloc) == (destination.scheme, destination.netloc):
            return redirected
        return urllib.request.Request(
            redirected.full_url,
            headers={"Accept": "application/octet-stream"},
            method=redirected.get_method(),
        )


def api_request(api_root: str, token: str, path: str, *, method: str = "GET", body: Any = None) -> Any:
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}{path}",
        data=data,
        method=method,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            if response.status == 204:
                return None
            return json.load(response)
    except Exception as exc:
        detail = getattr(exc, "code", "request-failed")
        raise PreparationError(f"GitHub API request failed: {detail}") from exc


def api_bytes(api_root: str, token: str, url: str) -> bytes:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.build_opener(RedirectHandler()).open(request, timeout=30) as response:
        return response.read()


def workflow_runs(api_root: str, repository: str, token: str, target_sha: str) -> list[dict[str, Any]]:
    query = urllib.parse.urlencode({"event": "push", "branch": "main", "head_sha": target_sha, "per_page": "100"})
    payload = api_request(api_root, token, f"/repos/{repository}/actions/workflows/{WORKFLOW_FILE}/runs?{query}")
    return [run for run in payload.get("workflow_runs", []) if isinstance(run, dict)]


def recovery_runs(api_root: str, repository: str, token: str, recovery_request: str) -> list[dict[str, Any]]:
    query = urllib.parse.urlencode({"event": "workflow_dispatch", "branch": "main", "per_page": "100"})
    payload = api_request(api_root, token, f"/repos/{repository}/actions/workflows/{WORKFLOW_FILE}/runs?{query}")
    return [
        run
        for run in payload.get("workflow_runs", [])
        if isinstance(run, dict) and recovery_request in str(run.get("display_title", ""))
    ]


def recovery_run_uses_current_artifact_contract(
    api_root: str,
    repository: str,
    token: str,
    run: dict[str, Any],
) -> bool:
    head_sha = run.get("head_sha")
    if not isinstance(head_sha, str) or not SHA_RE.fullmatch(head_sha):
        raise PreparationError("recovery preparation run has an invalid head SHA")
    comparison = api_request(
        api_root,
        token,
        f"/repos/{repository}/compare/{ARTIFACT_UPLOAD_CONTRACT_SHA}...{head_sha}",
    )
    status = comparison.get("status") if isinstance(comparison, dict) else None
    if status not in {"ahead", "behind", "identical", "diverged"}:
        raise PreparationError("recovery preparation contract comparison is invalid")
    if status == "diverged":
        raise PreparationError("recovery preparation contract comparison is not a linear ancestry")
    return status in {"ahead", "identical"}


def manifest_from_run(api_root: str, repository: str, token: str, run: dict[str, Any], target_sha: str) -> dict[str, Any] | None:
    run_id = run.get("id")
    if not isinstance(run_id, int):
        return None
    payload = api_request(api_root, token, f"/repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100")
    artifact_name = f"{ARTIFACT_PREFIX}{target_sha}"
    for artifact in payload.get("artifacts", []):
        if artifact.get("name") != artifact_name or artifact.get("expired"):
            continue
        archive = api_bytes(api_root, token, artifact["archive_download_url"])
        with zipfile.ZipFile(BytesIO(archive)) as bundle:
            for member in bundle.namelist():
                if member.endswith("release-preparation-manifest.json"):
                    return json.loads(bundle.read(member))
    return None


def successful_manifest(api_root: str, repository: str, token: str, runs: list[dict[str, Any]], target_sha: str) -> tuple[dict[str, Any], dict[str, Any]] | None:
    for run in sorted(runs, key=lambda item: item.get("id", 0), reverse=True):
        if run.get("status") != "completed" or run.get("conclusion") != "success":
            continue
        manifest = manifest_from_run(api_root, repository, token, run, target_sha)
        if manifest is None:
            continue
        valid, reason = validate_manifest(manifest, target_sha)
        if valid:
            return run, manifest
        raise PreparationError(f"preparation manifest rejected: {reason}")
    return None


def wait_for_manifest(
    api_root: str,
    repository: str,
    token: str,
    target_sha: str,
    *,
    recovery_request: str = "",
    timeout_seconds: int,
    poll_seconds: int,
    ignored_failed_run_ids: set[int] | None = None,
    wait_for_start: bool = False,
) -> tuple[dict[str, Any], dict[str, Any]] | None:
    ignored_failed_run_ids = ignored_failed_run_ids or set()
    deadline = time.monotonic() + timeout_seconds
    while True:
        runs = recovery_runs(api_root, repository, token, recovery_request) if recovery_request else workflow_runs(api_root, repository, token, target_sha)
        found = successful_manifest(api_root, repository, token, runs, target_sha)
        if found:
            return found
        failed = [
            run
            for run in runs
            if run.get("status") == "completed"
            and run.get("conclusion") != "success"
            and run.get("id") not in ignored_failed_run_ids
        ]
        active = [run for run in runs if run.get("status") in {"queued", "in_progress"}]
        if failed:
            raise PreparationError(f"preparation workflow failed for {target_sha}")
        if not active:
            if wait_for_start and time.monotonic() < deadline:
                wait_for_start = False
                time.sleep(poll_seconds)
                continue
            return None
        if time.monotonic() >= deadline:
            return None
        time.sleep(poll_seconds)


def ensure_preparation(args: argparse.Namespace) -> dict[str, Any]:
    if not SHA_RE.fullmatch(args.target_sha):
        raise PreparationError("target SHA is invalid")
    started = time.monotonic()
    try:
        normal = wait_for_manifest(
            args.api_root,
            args.repository,
            args.token,
            args.target_sha,
            timeout_seconds=args.timeout_seconds,
            poll_seconds=args.poll_seconds,
        )
    except PreparationError as error:
        if "preparation workflow failed" not in str(error):
            raise
        normal = None
    if normal:
        run, manifest = normal
        return {
            "target_sha": args.target_sha,
            "run_id": run["id"],
            "artifact_name": f"{ARTIFACT_PREFIX}{args.target_sha}",
            "recovery": False,
            "manifest_sha256": manifest_digest(manifest),
        }

    recovery_request = f"release-recovery-{args.target_sha}"
    print(f"::warning::release preparation artifact is missing or expired for {args.target_sha}; dispatching one recovery build", file=os.sys.stderr)
    existing = recovery_runs(args.api_root, args.repository, args.token, recovery_request)
    recovered = successful_manifest(args.api_root, args.repository, args.token, existing, args.target_sha)
    if recovered:
        run, manifest = recovered
        return {
            "target_sha": args.target_sha,
            "run_id": run["id"],
            "artifact_name": f"{ARTIFACT_PREFIX}{args.target_sha}",
            "recovery": True,
            "recovery_request": recovery_request,
            "manifest_sha256": manifest_digest(manifest),
        }

    legacy_failed_run_ids: set[int] = set()
    completed_runs = [run for run in existing if run.get("status") == "completed"]
    for run in completed_runs:
        run_id = run.get("id")
        if not isinstance(run_id, int):
            raise PreparationError("recovery preparation run has an invalid id")
        if run.get("conclusion") == "success" or recovery_run_uses_current_artifact_contract(
            args.api_root,
            args.repository,
            args.token,
            run,
        ):
            raise PreparationError("the single recovery preparation run already failed")

        legacy_failed_run_ids.add(run_id)

    active = [run for run in existing if run.get("status") in {"queued", "in_progress"}]
    dispatched_recovery = False
    if not active:
        api_request(
            args.api_root,
            args.token,
            f"/repos/{args.repository}/actions/workflows/{WORKFLOW_FILE}/dispatches",
            method="POST",
            body={"ref": "main", "inputs": {"target_sha": args.target_sha, "recovery_request": recovery_request}},
        )
        dispatched_recovery = True
    remaining = args.timeout_seconds - int(time.monotonic() - started)
    if remaining <= 0:
        raise PreparationError(f"preparation recovery budget of {args.timeout_seconds}s was exhausted")
    recovered = wait_for_manifest(
        args.api_root,
        args.repository,
        args.token,
        args.target_sha,
        recovery_request=recovery_request,
        timeout_seconds=remaining,
        poll_seconds=args.poll_seconds,
        ignored_failed_run_ids=legacy_failed_run_ids,
        wait_for_start=dispatched_recovery,
    )
    if not recovered:
        raise PreparationError(f"recovery preparation did not finish within {args.timeout_seconds}s")
    run, manifest = recovered
    return {
        "target_sha": args.target_sha,
        "run_id": run["id"],
        "artifact_name": f"{ARTIFACT_PREFIX}{args.target_sha}",
        "recovery": True,
        "recovery_request": recovery_request,
        "manifest_sha256": manifest_digest(manifest),
    }


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
    manifest.add_argument("--recovery-request", default="")
    manifest.add_argument("--output", required=True)
    validate = sub.add_parser("validate")
    validate.add_argument("--manifest", type=Path, required=True)
    validate.add_argument("--target-sha", required=True)
    validate.add_argument("--allow-recovery", action="store_true")
    verify = sub.add_parser("verify")
    verify.add_argument("--root", default=".")
    verify.add_argument("--manifest", type=Path, required=True)
    verify.add_argument("--target-sha", required=True)
    verify.add_argument("--allow-recovery", action="store_true")
    verify.add_argument("--expected-manifest-sha256")
    ensure = sub.add_parser("ensure")
    ensure.add_argument("--repository", required=True)
    ensure.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))
    ensure.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    ensure.add_argument("--target-sha", required=True)
    ensure.add_argument("--timeout-seconds", type=int, default=720)
    ensure.add_argument("--poll-seconds", type=int, default=15)
    ensure.add_argument("--output", type=Path, required=True)
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
                recovery_request=args.recovery_request,
            )
            Path(args.output).write_text(json.dumps(payload, sort_keys=True) + "\n")
            return 0
        if args.command == "validate":
            payload = json.loads(args.manifest.read_text())
            valid, reason = validate_manifest(payload, args.target_sha, allow_recovery=args.allow_recovery)
            print(reason)
            return 0 if valid else 1
        if args.command == "verify":
            payload = json.loads(args.manifest.read_text())
            valid, reason = verify_manifest_files(
                Path(args.root),
                payload,
                args.target_sha,
                allow_recovery=args.allow_recovery,
                expected_manifest_sha256=args.expected_manifest_sha256,
            )
            print(reason)
            return 0 if valid else 1
        if not args.token:
            raise PreparationError("ensure requires a GitHub token")
        result = ensure_preparation(args)
        args.output.write_text(json.dumps(result, sort_keys=True) + "\n")
        print(json.dumps(result, sort_keys=True))
        return 0
    except PreparationError as error:
        print(str(error), file=os.sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
