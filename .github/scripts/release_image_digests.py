#!/usr/bin/env python3
"""Record and verify the container image digests published for one release identity."""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

import release_artifact_bundle
import release_policy


class ImageDigestError(ValueError):
    pass


def canonical_digest(value: str, label: str) -> str:
    if not isinstance(value, str) or not re.fullmatch(r"(?:sha256:)?[0-9a-f]{64}", value):
        raise ImageDigestError(f"{label} is missing or invalid")
    return value if value.startswith("sha256:") else f"sha256:{value}"


def build_payload(identity: dict[str, Any], images: dict[str, str]) -> dict[str, Any]:
    normalized = {
        name: canonical_digest(digest, f"{name} image digest") for name, digest in images.items()
    }
    release_policy.validate_image_digests(normalized)
    return {
        "schema_version": 1,
        "identity": release_artifact_bundle.identity_projection(identity),
        "images": normalized,
    }


def validate_payload(payload: dict[str, Any], identity: dict[str, Any] | None = None) -> dict[str, Any]:
    if not isinstance(payload, dict) or payload.get("schema_version") != 1:
        raise ImageDigestError("unsupported release image digest record")
    images = release_policy.validate_image_digests(payload.get("images"))
    if identity is not None:
        expected = release_artifact_bundle.identity_projection(identity)
        if payload.get("identity") != expected:
            raise ImageDigestError("release image digests belong to a different immutable identity")
    return {"identity": payload.get("identity"), "images": images}


def api_json(api_root: str, token: str, path: str) -> Any:
    request = urllib.request.Request(
        f"{api_root.rstrip('/')}{path}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "dockrev-release-image-digests",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.loads(response.read().decode(response.headers.get_content_charset() or "utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        raise ImageDigestError(f"GitHub API request failed: {error.code}: {detail[:400]}") from error
    except urllib.error.URLError as error:
        raise ImageDigestError(f"GitHub API request failed: {error.reason}") from error


def published_digest(
    api_root: str, token: str, owner: str, package: str, version: str,
) -> str:
    page = 1
    while page <= 20:
        payload = api_json(
            api_root, token,
            f"/users/{urllib.parse.quote(owner, safe='')}/packages/container/"
            f"{urllib.parse.quote(package, safe='')}/versions?per_page=100&page={page}",
        )
        if not isinstance(payload, list):
            raise ImageDigestError(f"GitHub did not return container versions for {package}")
        for item in payload:
            if not isinstance(item, dict):
                continue
            tags = (item.get("metadata") or {}).get("container", {}).get("tags") or []
            if version in tags:
                return canonical_digest(str(item.get("name", "")), f"{package} published digest")
        if len(payload) < 100:
            break
        page += 1
    raise ImageDigestError(f"package {package} has no published version tagged {version}")


def verify_remote(
    api_root: str, token: str, repository: str, version: str, images: dict[str, str],
) -> dict[str, Any]:
    release_policy.parse_version(version)
    owner = repository.partition("/")[0]
    if not owner:
        raise ImageDigestError("repository must be owner/name")
    verified = {}
    for package, expected in sorted(images.items()):
        actual = published_digest(api_root, token, owner.lower(), package, version)
        if actual != expected:
            raise ImageDigestError(
                f"package {package}:{version} resolves to {actual}, not the recorded identity digest {expected}"
            )
        verified[package] = actual
    return {"version": version, "images": verified}


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    create = subparsers.add_parser("create")
    create.add_argument("--identity", type=Path, required=True)
    create.add_argument("--output", type=Path, required=True)
    create.add_argument("--recorded", default="")
    create.add_argument("--dockrev-digest", default="")
    create.add_argument("--supervisor-digest", default="")

    verify = subparsers.add_parser("verify")
    verify.add_argument("--path", type=Path, required=True)
    verify.add_argument("--identity", type=Path, default=None)

    remote = subparsers.add_parser("verify-remote")
    remote.add_argument("--repository", required=True)
    remote.add_argument("--version", required=True)
    remote.add_argument("--path", type=Path, required=True)
    remote.add_argument("--identity", type=Path, default=None)
    remote.add_argument("--token", default="")
    remote.add_argument("--api-root", default="https://api.github.com")

    args = parser.parse_args()
    try:
        identity = json.loads(args.identity.read_text(encoding="utf-8")) if getattr(args, "identity", None) else None
        if args.command == "create":
            if args.recorded.strip():
                recorded = json.loads(args.recorded)
                images = release_policy.validate_image_digests(recorded.get("images", recorded))
            else:
                images = {
                    "dockrev": canonical_digest(args.dockrev_digest, "dockrev image digest"),
                    "dockrev-supervisor": canonical_digest(args.supervisor_digest, "dockrev-supervisor image digest"),
                }
            if identity is None:
                raise ImageDigestError("--identity is required to record image digests")
            payload = build_payload(identity, images)
            args.output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            print(json.dumps(payload, sort_keys=True))
            return 0
        stored = json.loads(args.path.read_text(encoding="utf-8"))
        if isinstance(stored, dict) and "images" not in stored:
            stored = {"schema_version": 1, "images": stored}
        validated = validate_payload(stored, identity)
        if args.command == "verify":
            print(json.dumps(validated, sort_keys=True))
            return 0
        if not args.token:
            parser.error("--token is required for verify-remote")
        print(json.dumps(verify_remote(args.api_root, args.token, args.repository, args.version, validated["images"]), sort_keys=True))
        return 0
    except (ImageDigestError, release_policy.PolicyError, OSError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
