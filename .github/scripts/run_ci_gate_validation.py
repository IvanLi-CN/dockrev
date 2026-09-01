#!/usr/bin/env python3
"""Run the fixed, serial CI Gate Verification acceptance matrix."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path
from typing import Any


SHA_RE = re.compile(r"[0-9a-f]{40}")
RUN_URL_RE = re.compile(r"/actions/runs/(\d+)(?:\D|$)")
TIMESTAMP_RE = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z")
WORKFLOW = "ci-gate-verification.yml"
TOTAL_DISPATCHES = 17


def parse_utc(value: Any, key: str) -> datetime:
    if not isinstance(value, str) or not TIMESTAMP_RE.fullmatch(value):
        raise ValueError(f"{key} must be an ISO-8601 UTC timestamp ending in Z")
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def validate_sha(value: str, label: str) -> str:
    if not SHA_RE.fullmatch(value):
        raise ValueError(f"{label} must be a 40-character lowercase commit SHA")
    return value


def invoke(
    command: list[str], *, capture: bool = False, timeout_seconds: int | None = None
) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            command,
            check=False,
            text=True,
            capture_output=capture,
            timeout=timeout_seconds,
        )
    except subprocess.TimeoutExpired as error:
        raise RuntimeError(f"command timed out after {timeout_seconds}s") from error


def dispatch(repository: str, ref: str, target_sha: str) -> int:
    result = invoke(
        [
            "gh",
            "workflow",
            "run",
            WORKFLOW,
            "--repo",
            repository,
            "--ref",
            ref,
            "--raw-field",
            f"target_sha={target_sha}",
        ],
        capture=True,
        timeout_seconds=60,
    )
    if result.returncode != 0:
        raise RuntimeError(f"workflow dispatch failed with exit {result.returncode}")
    output = f"{result.stdout}\n{result.stderr}"
    match = RUN_URL_RE.search(output)
    if not match:
        raise RuntimeError("gh workflow run did not return a run URL")
    return int(match.group(1))


def watch(repository: str, run_id: int, timeout_seconds: int, interval_seconds: int) -> None:
    timeout_binary = shutil.which("timeout")
    watch_command = [
        "gh",
        "run",
        "watch",
        str(run_id),
        "--repo",
        repository,
        "--interval",
        str(interval_seconds),
        "--compact",
        "--exit-status",
    ]
    if timeout_binary is None:
        result = invoke(watch_command, capture=True, timeout_seconds=timeout_seconds)
    else:
        result = invoke(
            [timeout_binary, "--signal=TERM", f"{timeout_seconds}s", *watch_command],
            capture=True,
        )
    if result.returncode != 0:
        raise RuntimeError(f"run {run_id} did not complete successfully (exit {result.returncode})")


def download_metrics(repository: str, run_id: int, directory: Path) -> dict[str, Any]:
    directory.mkdir(parents=True, exist_ok=False)
    result = invoke(
        [
            "gh",
            "run",
            "download",
            str(run_id),
            "--repo",
            repository,
            "--name",
            f"ci-gate-metrics-{run_id}",
            "--dir",
            str(directory),
        ],
        capture=True,
        timeout_seconds=120,
    )
    if result.returncode != 0:
        raise RuntimeError(f"run {run_id} metrics artifact download failed")
    files = sorted(directory.rglob("*.json"))
    if len(files) != 1:
        raise RuntimeError(f"run {run_id} must contain exactly one metrics JSON artifact")
    return json.loads(files[0].read_text())


def validate_sample(payload: dict[str, Any], target_sha: str, expected_cache: str | None) -> None:
    required = {
        "target_sha": target_sha,
        "fast_target_sha": target_sha,
        "scope": "full",
        "web": True,
        "docker": True,
        "publish": False,
        "fast_result": "success",
        "source_result": "success",
        "coverage_result": "success",
        "verification_mode": True,
    }
    for key, expected in required.items():
        if payload.get(key) != expected:
            raise ValueError(f"metric {key}={payload.get(key)!r} expected {expected!r}")
    if payload.get("cache_status") not in {"cold", "warm"}:
        raise ValueError("metric cache_status must be cold or warm")
    if expected_cache is not None and payload.get("cache_status") != expected_cache:
        raise ValueError(f"metric cache_status must be {expected_cache}")
    if not isinstance(payload.get("storybook_shard_total"), int) or payload["storybook_shard_total"] not in (2, 3):
        raise ValueError("metric must include a two- or three-shard Storybook total")
    if not isinstance(payload.get("storybook_story_count"), int) or payload["storybook_story_count"] <= 0:
        raise ValueError("metric must include a positive Storybook story count")
    if not re.fullmatch(r"[0-9a-f]{64}", str(payload.get("coverage_summary", ""))):
        raise ValueError("metric must include a SHA-256 Storybook coverage summary")

    durations = [payload.get(key) for key in ("queue_seconds", "fast_seconds", "source_seconds", "eligibility_seconds", "execution_seconds")]
    if any(not isinstance(value, (int, float)) or value < 0 for value in durations):
        raise ValueError("metric durations must be non-negative numbers")
    if payload["execution_seconds"] > 600:
        raise ValueError("sample exceeds the 600 second investigation threshold")

    created = parse_utc(payload.get("created_at"), "created_at")
    started = parse_utc(payload.get("run_started_at"), "run_started_at")
    fast = parse_utc(payload.get("fast_completed_at"), "fast_completed_at")
    source = parse_utc(payload.get("source_completed_at"), "source_completed_at")
    eligibility = parse_utc(payload.get("eligibility_completed_at"), "eligibility_completed_at")
    if started < created or fast < started or source < started or eligibility != max(fast, source):
        raise ValueError("metric timestamps are out of order")
    expected_durations = {
        "queue_seconds": (started - created).total_seconds(),
        "fast_seconds": (fast - started).total_seconds(),
        "source_seconds": (source - started).total_seconds(),
        "eligibility_seconds": (eligibility - started).total_seconds(),
        "execution_seconds": (eligibility - started).total_seconds(),
    }
    for key, expected in expected_durations.items():
        if abs(float(payload[key]) - expected) > 1e-6:
            raise ValueError(f"metric {key} does not match its UTC timestamps")


def build_cases(args: argparse.Namespace) -> list[tuple[str, str, str, int, str | None]]:
    return [
        *[("two-shard", args.two_shard_sha, args.two_shard_ref, 2, None) for _ in range(3)],
        *[("three-shard", args.three_shard_sha, args.three_shard_ref, 3, None) for _ in range(3)],
        ("cold-warmup", args.final_sha, args.final_ref, args.final_shards, "cold"),
        *[("warm", args.final_sha, args.final_ref, args.final_shards, "warm") for _ in range(10)],
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--two-shard-sha", required=True)
    parser.add_argument("--two-shard-ref", required=True)
    parser.add_argument("--three-shard-sha", required=True)
    parser.add_argument("--three-shard-ref", required=True)
    parser.add_argument("--final-sha", required=True)
    parser.add_argument("--final-ref", required=True)
    parser.add_argument("--final-shards", type=int, choices=(2, 3), required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--timeout-seconds", type=int, default=720)
    parser.add_argument("--interval-seconds", type=int, default=15)
    args = parser.parse_args()

    if args.timeout_seconds != 720 or args.interval_seconds != 15:
        parser.error("the acceptance contract fixes timeout-seconds=720 and interval-seconds=15")
    if args.output_dir.exists():
        parser.error("output-dir must not already exist; use a new directory for each matrix")
    for name in ("two_shard_sha", "three_shard_sha", "final_sha"):
        setattr(args, name, validate_sha(getattr(args, name), name))
    args.output_dir.mkdir(parents=True)

    records: list[dict[str, Any]] = []
    cases = build_cases(args)
    if len(cases) != TOTAL_DISPATCHES:
        raise RuntimeError("internal validation matrix must contain exactly 17 dispatches")

    matrix_deadline = time.monotonic() + TOTAL_DISPATCHES * args.timeout_seconds
    for index, (phase, target_sha, ref, expected_shards, expected_cache) in enumerate(cases, start=1):
        run_dir = args.output_dir / f"{index:02d}-{phase}"
        try:
            if time.monotonic() >= matrix_deadline:
                raise RuntimeError("17-run validation budget of 204 minutes has elapsed")
            run_id = dispatch(args.repository, ref, target_sha)
            watch(args.repository, run_id, args.timeout_seconds, args.interval_seconds)
            payload = download_metrics(args.repository, run_id, run_dir)
            if time.monotonic() >= matrix_deadline:
                raise RuntimeError("17-run validation budget of 204 minutes has elapsed")
            validate_sample(payload, target_sha, expected_cache)
            if payload["storybook_shard_total"] != expected_shards:
                raise ValueError(
                    f"expected {expected_shards} Storybook shards, got {payload['storybook_shard_total']}"
                )
            records.append(
                {
                    "index": index,
                    "phase": phase,
                    "target_sha": target_sha,
                    "ref": ref,
                    "run_id": run_id,
                    "cache_status": payload["cache_status"],
                    "storybook_shard_total": payload["storybook_shard_total"],
                    "fast_seconds": payload["fast_seconds"],
                    "source_seconds": payload["source_seconds"],
                    "eligibility_seconds": payload["eligibility_seconds"],
                }
            )
        except Exception as error:
            (args.output_dir / "failure.json").write_text(
                json.dumps(
                    {"failed_index": index, "phase": phase, "target_sha": target_sha, "error": str(error)},
                    sort_keys=True,
                )
                + "\n"
            )
            print(f"validation stopped at sample {index}/{TOTAL_DISPATCHES}: {error}", file=sys.stderr)
            return 1

    warm_dir = args.output_dir / "warm-metrics"
    warm_dir.mkdir()
    for record in records[-10:]:
        source = args.output_dir / f"{record['index']:02d}-warm" / "ci-gate-metrics.json"
        if not source.is_file():
            candidates = sorted((args.output_dir / f"{record['index']:02d}-warm").rglob("*.json"))
            if len(candidates) != 1:
                raise RuntimeError(f"missing warm metrics for sample {record['index']}")
            source = candidates[0]
        shutil.copy2(source, warm_dir / f"{record['index']:02d}.json")

    verifier = invoke(
        [sys.executable, str(Path(__file__).with_name("verify_ci_gate_metrics.py")), str(warm_dir)],
        capture=True,
    )
    (args.output_dir / "final-metrics.json").write_text(verifier.stdout)
    if verifier.returncode != 0:
        print("final ten warm samples failed timing acceptance", file=sys.stderr)
        return 1

    (args.output_dir / "matrix.json").write_text(json.dumps({"dispatch_count": len(records), "records": records}, sort_keys=True) + "\n")
    print(verifier.stdout, end="")
    print(f"controlled validation passed: {len(records)} serial runs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
