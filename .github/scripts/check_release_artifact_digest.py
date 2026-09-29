#!/usr/bin/env python3
"""Verify an immutable release artifact against GitHub artifact metadata."""

from __future__ import annotations

import argparse
import json
import re
import sys
from typing import Any


SHA256_RE = re.compile(r"^(?:sha256:)?([0-9a-f]{64})$")


class ArtifactDigestError(ValueError):
    pass


def canonical_digest(value: Any, label: str) -> str:
    if not isinstance(value, str):
        raise ArtifactDigestError(f"{label} is not a SHA-256 digest")
    match = SHA256_RE.fullmatch(value)
    if match is None:
        raise ArtifactDigestError(f"{label} is not a SHA-256 digest")
    return f"sha256:{match.group(1)}"


def verify_artifact(
    metadata: Any, *, artifact_name: str, expected_digest: str,
) -> dict[str, Any]:
    if not isinstance(artifact_name, str) or not artifact_name:
        raise ArtifactDigestError("artifact name is missing")
    if not isinstance(metadata, dict) or not isinstance(metadata.get("artifacts"), list):
        raise ArtifactDigestError("GitHub artifact metadata is invalid")

    matches = [
        artifact
        for artifact in metadata["artifacts"]
        if isinstance(artifact, dict) and artifact.get("name") == artifact_name
    ]
    if len(matches) != 1:
        raise ArtifactDigestError("GitHub artifact metadata does not contain exactly one named bundle")

    artifact = matches[0]
    if artifact.get("expired") is not False:
        raise ArtifactDigestError("release artifact is expired or has an invalid expiration state")
    actual = canonical_digest(artifact.get("digest"), "GitHub artifact digest")
    expected = canonical_digest(expected_digest, "expected artifact digest")
    if actual != expected:
        raise ArtifactDigestError("release artifact digest does not match the immutable identity bundle")
    return artifact


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-name", required=True)
    parser.add_argument("--expected-digest", required=True)
    args = parser.parse_args()
    try:
        metadata = json.load(sys.stdin)
        verify_artifact(
            metadata,
            artifact_name=args.artifact_name,
            expected_digest=args.expected_digest,
        )
    except (ArtifactDigestError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
