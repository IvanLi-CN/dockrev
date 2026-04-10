#!/usr/bin/env python3
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

RULES = (
    ("crates/", {".rs"}, 1500),
    ("web/src/", {".ts", ".tsx"}, 1200),
    ("web/tests/", {".ts", ".tsx"}, 1000),
)


def tracked_files(repo_root: Path) -> list[str]:
    output = subprocess.check_output(["git", "ls-files"], cwd=repo_root, text=True)
    return [line for line in output.splitlines() if line]


def match_rule(path: str):
    for prefix, suffixes, budget in RULES:
        if path.startswith(prefix) and Path(path).suffix in suffixes:
            return budget
    return None


def line_count(path: Path) -> int:
    with path.open("r", encoding="utf-8") as handle:
        return sum(1 for _ in handle)


def main() -> int:
    parser = argparse.ArgumentParser(description="Check tracked source files against repo line budgets.")
    parser.add_argument("--repo-root", default=".", help="Repository root (defaults to current directory)")
    args = parser.parse_args()

    repo_root = Path(args.repo_root).resolve()
    violations: list[tuple[int, int, str]] = []

    for rel_path in tracked_files(repo_root):
        budget = match_rule(rel_path)
        if budget is None:
            continue
        abs_path = repo_root / rel_path
        if not abs_path.is_file():
            continue
        count = line_count(abs_path)
        if count > budget:
            violations.append((count, budget, rel_path))

    if not violations:
        print("OK: no tracked source files exceed the configured line budgets.")
        return 0

    print("Line budget violations:")
    for count, budget, rel_path in sorted(violations, key=lambda item: (-item[0], item[2])):
        print(f"  {rel_path}: {count} lines (budget {budget})")
    print(f"Total violations: {len(violations)}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
