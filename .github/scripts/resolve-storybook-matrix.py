#!/usr/bin/env python3
"""Resolve the checked-in Storybook shard count into a matrix."""

from __future__ import annotations

import json
from pathlib import Path


def shard_total() -> int:
    config = Path(".github/storybook-shards.json")
    if not config.is_file():
        return 3
    payload = json.loads(config.read_text())
    total = payload.get("total", 3)
    if not isinstance(total, int) or total not in (2, 3):
        raise ValueError(".github/storybook-shards.json total must be 2 or 3")
    return total


def matrix() -> list[dict[str, str]]:
    total = shard_total()
    return ([{"id": "global", "role": "global", "shard": ""}] + [
        {"id": f"shard-{index}", "role": "shard", "shard": f"{index}/{total}"}
        for index in range(1, total + 1)
    ])


if __name__ == "__main__":
    print(f"matrix={json.dumps(matrix(), separators=(',', ':'))}")
