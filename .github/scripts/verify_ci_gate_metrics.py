#!/usr/bin/env python3
"""Verify deterministic CI gate timing and scope metrics."""

from __future__ import annotations

import argparse
import json
import re
from datetime import datetime, timezone
from math import ceil
from pathlib import Path
from typing import Any


def percentile(values: list[float], percentile_rank: float) -> float:
    ordered = sorted(values)
    if percentile_rank == 0.5 and len(ordered) % 2 == 0:
        middle = len(ordered) // 2
        return (ordered[middle - 1] + ordered[middle]) / 2
    index = max(0, min(len(ordered) - 1, ceil(len(ordered) * percentile_rank) - 1))
    return ordered[index]


def load_metrics(directory: Path) -> list[dict[str, Any]]:
    files = sorted(directory.glob("*.json"))
    metrics = [json.loads(path.read_text()) for path in files]
    if not metrics:
        raise ValueError("no metrics JSON files found")
    return metrics


def parse_utc(value: Any, key: str) -> datetime:
    if not isinstance(value, str) or not re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", value):
        raise ValueError(f"{key} must be an ISO-8601 UTC timestamp ending in Z")
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo != timezone.utc:
        raise ValueError(f"{key} must use UTC")
    return parsed


def verify(metrics: list[dict[str, Any]], expected_count: int = 10) -> dict[str, float]:
    if len(metrics) != expected_count:
        raise ValueError(f"expected {expected_count} metrics, got {len(metrics)}")
    targets = {item.get("target_sha") for item in metrics}
    if len(targets) != 1 or not next(iter(targets), "") or not re.fullmatch(r"[0-9a-f]{40}", next(iter(targets), "")):
        raise ValueError("every metric must use the same 40-character lowercase target SHA")
    for item in metrics:
        if item.get("fast_target_sha") != next(iter(targets)):
            raise ValueError("fast gate must attest the same target SHA")
        if item.get("storybook_shard_total") not in (2, 3):
            raise ValueError("every metric must include a two- or three-shard Storybook total")
        if not isinstance(item.get("storybook_story_count"), int) or item["storybook_story_count"] <= 0:
            raise ValueError("every metric must include a positive Storybook story count")
        if not re.fullmatch(r"[0-9a-f]{64}", str(item.get("coverage_summary", ""))):
            raise ValueError("every metric must include a SHA-256 Storybook coverage summary")
        if item.get("scope") != "full" or item.get("web") is not True or item.get("docker") is not True:
            raise ValueError("every metric must prove full web/docker scope")
        if item.get("publish") is not False:
            raise ValueError("performance samples must prove publish=false")
        if item.get("fast_result") != "success" or item.get("source_result") != "success" or item.get("coverage_result") != "success":
            raise ValueError("performance samples must prove successful fast/source/coverage gates")
        if item.get("verification_mode") is not True:
            raise ValueError("performance samples must prove verification_mode=true")
        if item.get("cache_status") != "warm":
            raise ValueError("performance samples must have warm source cache")
        durations = [
            item.get(key)
            for key in (
                "queue_seconds",
                "fast_queue_seconds",
                "source_queue_seconds",
                "fast_seconds",
                "source_seconds",
                "eligibility_seconds",
                "execution_seconds",
                "wall_seconds",
            )
        ]
        if any(not isinstance(value, (int, float)) or value < 0 for value in durations):
            raise ValueError("metric durations must be non-negative numbers")
        if item.get("execution_seconds", 0) > 600:
            raise ValueError("sample exceeds 600 second investigation threshold")
        created = parse_utc(item.get("created_at"), "created_at")
        run_started = parse_utc(item.get("run_started_at"), "run_started_at")
        fast_started = parse_utc(item.get("fast_started_at"), "fast_started_at")
        source_started = parse_utc(item.get("source_started_at"), "source_started_at")
        fast_completed = parse_utc(item.get("fast_completed_at"), "fast_completed_at")
        source_completed = parse_utc(item.get("source_completed_at"), "source_completed_at")
        eligibility_completed = parse_utc(item.get("eligibility_completed_at"), "eligibility_completed_at")
        if (
            run_started < created
            or fast_started < run_started
            or source_started < run_started
            or fast_completed < fast_started
            or source_completed < source_started
        ):
            raise ValueError("metric timestamps are out of order")
        expected_eligibility = max(fast_completed, source_completed)
        if eligibility_completed != expected_eligibility:
            raise ValueError("eligibility_completed_at must be the later fast/source completion")
        expected_durations = {
            "queue_seconds": (run_started - created).total_seconds(),
            "fast_queue_seconds": (fast_started - run_started).total_seconds(),
            "source_queue_seconds": (source_started - run_started).total_seconds(),
            "fast_seconds": (fast_completed - fast_started).total_seconds(),
            "source_seconds": (source_completed - source_started).total_seconds(),
            "eligibility_seconds": (eligibility_completed - run_started).total_seconds(),
            "execution_seconds": max(
                (fast_completed - fast_started).total_seconds(),
                (source_completed - source_started).total_seconds(),
            ),
            "wall_seconds": (eligibility_completed - run_started).total_seconds(),
        }
        for key, expected in expected_durations.items():
            if abs(float(item[key]) - expected) > 1e-6:
                raise ValueError(f"{key} does not match its UTC timestamps")

    fast = [float(item["fast_seconds"]) for item in metrics]
    source = [float(item["source_seconds"]) for item in metrics]
    eligibility = [float(item["eligibility_seconds"]) for item in metrics]
    result = {
        "fast_p50": percentile(fast, 0.5),
        "fast_p90": percentile(fast, 0.9),
        "source_p50": percentile(source, 0.5),
        "source_p90": percentile(source, 0.9),
        "eligibility_p50": percentile(eligibility, 0.5),
        "eligibility_p90": percentile(eligibility, 0.9),
    }
    thresholds = {
        "fast_p50": 360,
        "fast_p90": 420,
        "source_p50": 390,
        "source_p90": 480,
        "eligibility_p50": 420,
        "eligibility_p90": 480,
    }
    failures = [f"{key}={result[key]:g}s>{limit}s" for key, limit in thresholds.items() if result[key] > limit]
    if failures:
        raise ValueError("timing thresholds failed: " + ", ".join(failures))
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("--expected-count", type=int, default=10)
    args = parser.parse_args()
    result = verify(load_metrics(args.directory), args.expected_count)
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
