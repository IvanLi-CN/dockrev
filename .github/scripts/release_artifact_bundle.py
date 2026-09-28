#!/usr/bin/env python3
"""Build and verify the single immutable binary/package release artifact."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any


IDENTITY_FIELDS = (
    "source_sha",
    "merge_commit_sha",
    "identity_sha",
    "baseline_version",
    "version_input",
    "version",
    "channel",
    "release_tag",
)


class BundleError(ValueError):
    pass


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def expected_paths(version: str) -> set[str]:
    paths = {
        f"release-assets/{arch}/{libc}/{binary}"
        for arch in ("amd64", "arm64")
        for libc in ("gnu", "musl")
        for binary in ("dockrev", "dockrev-supervisor")
    }
    for arch in ("amd64", "arm64"):
        for libc in ("gnu", "musl"):
            for binary in ("dockrev", "dockrev-supervisor"):
                base = f"{binary}_{version}_linux_{arch}_{libc}.tar.gz"
                paths.add(f"dist/release/{base}")
                paths.add(f"dist/release/{base}.sha256")
    return paths


def identity_projection(identity: dict[str, Any]) -> dict[str, str]:
    missing = [field for field in IDENTITY_FIELDS if not isinstance(identity.get(field), str) or not identity[field]]
    if missing:
        raise BundleError(f"release identity missing bundle fields: {', '.join(missing)}")
    return {field: str(identity[field]) for field in IDENTITY_FIELDS}


def file_index(root: Path) -> list[dict[str, Any]]:
    entries = []
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        relative = path.relative_to(root).as_posix()
        if relative == "manifest.json":
            continue
        entries.append({"path": relative, "sha256": sha256(path), "size": path.stat().st_size})
    return entries


def content_digest(entries: list[dict[str, Any]]) -> str:
    canonical = json.dumps(entries, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()


def create_bundle(
    release_assets: Path, release_files: Path, output: Path, identity: dict[str, Any],
) -> dict[str, Any]:
    version = identity_projection(identity)["version"]
    if output.exists():
        raise BundleError("release bundle output directory already exists")
    (output / "release-assets").parent.mkdir(parents=True, exist_ok=True)
    import shutil

    shutil.copytree(release_assets, output / "release-assets")
    shutil.copytree(release_files, output / "dist" / "release")
    entries = file_index(output)
    paths = {entry["path"] for entry in entries}
    missing = sorted(expected_paths(version) - paths)
    extra = sorted(paths - expected_paths(version))
    if missing or extra:
        raise BundleError(f"release bundle file set mismatch; missing={missing}, extra={extra}")
    manifest = {
        "schema_version": 1,
        "identity": identity_projection(identity),
        "files": entries,
        "content_digest": content_digest(entries),
    }
    (output / "manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return manifest


def verify_bundle(root: Path, expected_identity: dict[str, Any]) -> dict[str, Any]:
    try:
        manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise BundleError("release bundle manifest is missing or invalid") from error
    if not isinstance(manifest, dict) or manifest.get("schema_version") != 1:
        raise BundleError("unsupported release bundle manifest")
    expected = identity_projection(expected_identity)
    actual = manifest.get("identity")
    if actual != expected:
        raise BundleError("release bundle belongs to a different immutable identity")
    files = manifest.get("files")
    if not isinstance(files, list) or not all(isinstance(item, dict) for item in files):
        raise BundleError("release bundle file manifest is invalid")
    actual_files = file_index(root)
    if actual_files != files:
        raise BundleError("release bundle contents do not match their SHA-256 manifest")
    paths = {entry["path"] for entry in actual_files}
    required = expected_paths(expected["version"])
    if paths != required:
        raise BundleError("release bundle does not contain the complete binary and package set")
    expected_digest = content_digest(actual_files)
    if manifest.get("content_digest") != expected_digest:
        raise BundleError("release bundle content digest does not match its file manifest")
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    create = subparsers.add_parser("create")
    create.add_argument("--release-assets", type=Path, required=True)
    create.add_argument("--release-files", type=Path, required=True)
    create.add_argument("--output", type=Path, required=True)
    create.add_argument("--identity", type=Path, required=True)
    verify = subparsers.add_parser("verify")
    verify.add_argument("--root", type=Path, required=True)
    verify.add_argument("--identity", type=Path, required=True)
    args = parser.parse_args()
    try:
        identity = json.loads(args.identity.read_text(encoding="utf-8"))
        if args.command == "create":
            result = create_bundle(args.release_assets, args.release_files, args.output, identity)
        else:
            result = verify_bundle(args.root, identity)
        print(json.dumps(result, sort_keys=True))
    except (BundleError, OSError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
