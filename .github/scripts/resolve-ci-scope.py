#!/usr/bin/env python3
"""Resolve the main CI Docker/Web scope from an exact Git diff."""

from __future__ import annotations

import argparse
import fnmatch
import subprocess
from pathlib import PurePosixPath


DOCKER_PATTERNS = (
    "Dockerfile",
    ".github/**",
    "deploy/**",
    "Cargo.toml",
    "Cargo.lock",
    "crates/**",
    "src/**",
    "web/**",
)
WEB_PATTERNS = ("web/**",)


def matches(path: str, patterns: tuple[str, ...]) -> bool:
    normalized = PurePosixPath(path).as_posix()
    return any(fnmatch.fnmatchcase(normalized, pattern) for pattern in patterns)


def changed_paths(base_sha: str, head_sha: str) -> list[str]:
    if not base_sha or set(base_sha) == {"0"}:
        return ["<initial-push>"]
    result = subprocess.run(
        ["git", "diff", "--name-only", "--diff-filter=ACMRTUXB", base_sha, head_sha],
        check=True,
        capture_output=True,
        text=True,
    )
    return [line for line in result.stdout.splitlines() if line]


def resolve(base_sha: str, head_sha: str, force_full: bool) -> tuple[bool, bool, list[str]]:
    paths = [] if force_full else changed_paths(base_sha, head_sha)
    if force_full:
        return True, True, paths
    return (
        any(matches(path, WEB_PATTERNS) for path in paths),
        any(matches(path, DOCKER_PATTERNS) for path in paths),
        paths,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-sha", required=True)
    parser.add_argument("--head-sha", required=True)
    parser.add_argument("--full", action="store_true")
    args = parser.parse_args()
    web, docker, paths = resolve(args.base_sha, args.head_sha, args.full)
    print(f"web={'true' if web else 'false'}")
    print(f"docker={'true' if docker else 'false'}")
    print(f"changed_paths={len(paths)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
