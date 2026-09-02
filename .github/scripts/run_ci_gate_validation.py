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
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SHA_RE = re.compile(r"[0-9a-f]{40}")
RUN_URL_RE = re.compile(r"/actions/runs/(\d+)(?:\D|$)")
TIMESTAMP_RE = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z")
WORKFLOW = "ci-gate-verification.yml"
CANDIDATE_DISPATCHES = 6
FINAL_DISPATCHES = 10
TOTAL_DISPATCHES = CANDIDATE_DISPATCHES + FINAL_DISPATCHES
# The acceptance wall-clock budget remains fixed at 204 minutes.  It is a
# scheduling cap, not a derivation from the count of valid timing samples.
TOTAL_BUDGET_SECONDS = 204 * 60
WORKFLOW_TIMEOUT_SECONDS = 720
WARM_INVESTIGATION_THRESHOLD_SECONDS = 600
OBSERVATION_GRACE_SECONDS = 180
STATUS_QUERY_ATTEMPTS = 3
STATUS_QUERY_RETRY_SECONDS = 5


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


def remaining_before_deadline(deadlines: list[float]) -> float:
    remaining = min(deadlines) - time.time() if deadlines else float("inf")
    if remaining <= 0:
        raise RuntimeError("controlled validation budget of 204 minutes has elapsed")
    return remaining


def query_run_status(
    repository: str,
    run_id: int,
    deadlines: list[float],
) -> dict[str, Any]:
    """Read one run status with bounded transport retries and no new dispatch."""
    for attempt in range(STATUS_QUERY_ATTEMPTS):
        result = invoke(
            [
                "gh",
                "run",
                "view",
                str(run_id),
                "--repo",
                repository,
                "--json",
                "status,conclusion,startedAt",
            ],
            capture=True,
            timeout_seconds=60,
        )
        if result.returncode == 0:
            try:
                return json.loads(result.stdout)
            except json.JSONDecodeError as error:
                raise RuntimeError(f"run {run_id} status response was not valid JSON") from error
        if attempt + 1 < STATUS_QUERY_ATTEMPTS:
            time.sleep(min(STATUS_QUERY_RETRY_SECONDS, remaining_before_deadline(deadlines)))
    raise RuntimeError(
        f"run {run_id} status query failed after {STATUS_QUERY_ATTEMPTS} attempts"
    )


def watch(
    repository: str,
    run_id: int,
    timeout_seconds: int,
    interval_seconds: int,
    matrix_deadline: float | None = None,
    allow_completed_after_deadline: bool = False,
) -> None:
    """Poll a run to natural completion without cancelling at the metric deadline.

    The workflow's measured gate execution remains bounded by ``timeout_seconds``
    in ``validate_sample``. Runner queue time is outside that execution budget,
    so the observation grace starts only after GitHub reports ``startedAt``. The
    optional matrix deadline still bounds the whole serial acceptance run.
    """
    started_at: datetime | None = None
    observation_deadline: float | None = None
    while True:
        now = time.time()
        if matrix_deadline is not None and now >= matrix_deadline:
            raise RuntimeError("controlled validation budget of 204 minutes has elapsed")
        if observation_deadline is not None and now >= observation_deadline:
            raise RuntimeError(
                f"run {run_id} did not reach a terminal state within "
                f"{timeout_seconds}s plus {OBSERVATION_GRACE_SECONDS}s observation grace"
            )
        deadlines = [
            deadline
            for deadline in (matrix_deadline, observation_deadline)
            if deadline is not None
        ]
        status = query_run_status(repository, run_id, deadlines)
        reported_started_at = status.get("startedAt")
        if reported_started_at is None:
            if status.get("status") != "queued":
                raise RuntimeError(f"run {run_id} did not report startedAt after leaving the queue")
        else:
            parsed_started_at = parse_utc(reported_started_at, "startedAt")
            if started_at is not None and parsed_started_at != started_at:
                raise RuntimeError(f"run {run_id} reported inconsistent startedAt values")
            started_at = parsed_started_at
            observation_deadline = started_at.timestamp() + timeout_seconds + OBSERVATION_GRACE_SECONDS

        if status.get("status") == "completed":
            conclusion = status.get("conclusion")
            if conclusion != "success":
                raise RuntimeError(f"run {run_id} completed with conclusion {conclusion!r}")
            if not allow_completed_after_deadline and observation_deadline is not None:
                if time.time() >= observation_deadline:
                    raise RuntimeError(
                        f"run {run_id} did not reach a terminal state within "
                        f"{timeout_seconds}s plus {OBSERVATION_GRACE_SECONDS}s observation grace"
                    )
            return

        now = time.time()
        if matrix_deadline is not None and now >= matrix_deadline:
            raise RuntimeError("controlled validation budget of 204 minutes has elapsed")
        if observation_deadline is not None and now >= observation_deadline:
            raise RuntimeError(
                f"run {run_id} did not reach a terminal state within "
                f"{timeout_seconds}s plus {OBSERVATION_GRACE_SECONDS}s observation grace"
            )
        deadlines = [
            deadline
            for deadline in (matrix_deadline, observation_deadline)
            if deadline is not None
        ]
        remaining = remaining_before_deadline(deadlines) if deadlines else interval_seconds
        time.sleep(min(interval_seconds, remaining))


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


def validate_sample(
    payload: dict[str, Any],
    target_sha: str,
    expected_cache: str | None,
) -> None:
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
    if payload["execution_seconds"] > WORKFLOW_TIMEOUT_SECONDS:
        raise ValueError("sample exceeds the 720 second workflow timeout")
    if expected_cache == "warm" and payload["execution_seconds"] > WARM_INVESTIGATION_THRESHOLD_SECONDS:
        raise ValueError("warm sample exceeds the 600 second investigation threshold")

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


def build_cases(
    args: argparse.Namespace, phase: str, final_shards: int | None = None
) -> list[tuple[str, str, str, int, str | None]]:
    candidates = [
        ("two-shard", args.two_shard_sha, args.two_shard_ref, 2, None) for _ in range(3)
    ]
    candidates += [
        ("three-shard", args.three_shard_sha, args.three_shard_ref, 3, None) for _ in range(3)
    ]
    if phase == "candidates":
        return candidates
    if final_shards not in (2, 3):
        raise ValueError("final phase requires a selected two- or three-shard matrix")
    # Candidate verification and final acceptance use the same exact-SHA
    # verification cache scope. A separate final "cold" run would therefore
    # be impossible after the selected candidate has already completed.
    final = [("warm", args.final_sha, args.final_ref, final_shards, "warm") for _ in range(10)]
    return candidates + final if phase == "all" else final


def validate_resume_run_ids(run_ids: list[int]) -> None:
    if len(run_ids) > FINAL_DISPATCHES:
        raise ValueError(f"at most {FINAL_DISPATCHES} final runs can be resumed")
    if any(run_id <= 0 for run_id in run_ids):
        raise ValueError("each resumed GitHub run id must be positive")
    if len(set(run_ids)) != len(run_ids):
        raise ValueError("resumed GitHub run ids must be unique")


def select_final_matrix(records: list[dict[str, Any]]) -> dict[str, Any]:
    grouped = {
        2: [record for record in records if record.get("phase") == "two-shard"],
        3: [record for record in records if record.get("phase") == "three-shard"],
    }
    if any(len(grouped[shards]) != 3 for shards in (2, 3)):
        raise ValueError("candidate phase must contain exactly three runs for each shard matrix")
    p90 = {shards: sorted(record["fast_seconds"] for record in grouped[shards])[2] for shards in (2, 3)}
    difference = abs(p90[2] - p90[3])
    if difference < 30:
        selected_shards = 3
        reason = "fast P90 difference below 30 seconds; preserve three-shard coverage"
    else:
        selected_shards = min((2, 3), key=lambda shards: p90[shards])
        reason = "lower fast P90 wins"
    selected = next(record for record in grouped[selected_shards])
    return {
        "selected_shards": selected_shards,
        "selected_sha": selected["target_sha"],
        "selected_ref": selected["ref"],
        "two_shard_fast_p90": p90[2],
        "three_shard_fast_p90": p90[3],
        "p90_difference_seconds": difference,
        "reason": reason,
    }


def write_deadline(path: Path) -> float:
    deadline = time.time() + TOTAL_BUDGET_SECONDS
    path.write_text(
        json.dumps(
            {
                "budget_seconds": TOTAL_BUDGET_SECONDS,
                "deadline_epoch": deadline,
                "deadline_utc": datetime.fromtimestamp(deadline, timezone.utc)
                .isoformat()
                .replace("+00:00", "Z"),
            },
            sort_keys=True,
        )
        + "\n"
    )
    return deadline


def read_deadline(path: Path) -> float:
    try:
        payload = json.loads(path.read_text())
        deadline = float(payload["deadline_epoch"])
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        raise ValueError("candidate phase deadline artifact is invalid") from error
    if payload.get("budget_seconds") != TOTAL_BUDGET_SECONDS or deadline <= 0:
        raise ValueError("candidate phase deadline artifact does not match the 204-minute budget")
    return deadline


def read_candidate_matrix(directory: Path) -> tuple[list[dict[str, Any]], dict[str, Any], float]:
    try:
        payload = json.loads((directory / "matrix.json").read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError("candidate phase matrix artifact is missing or invalid") from error
    records = payload.get("records")
    if payload.get("phase") != "candidates" or payload.get("dispatch_count") != CANDIDATE_DISPATCHES:
        raise ValueError("candidate phase matrix must contain exactly six dispatches")
    if not isinstance(records, list) or len(records) != CANDIDATE_DISPATCHES:
        raise ValueError("candidate phase matrix records are incomplete")
    selection = select_final_matrix(records)
    return records, selection, read_deadline(directory / "deadline.json")


def run_cases(
    args: argparse.Namespace,
    cases: list[tuple[str, str, str, int, str | None]],
    start_index: int,
    deadline: float,
    records: list[dict[str, Any]],
) -> bool:
    for offset, (phase, target_sha, ref, expected_shards, expected_cache) in enumerate(cases):
        index = start_index + offset
        run_dir = args.output_dir / f"{index:02d}-{phase}"
        run_id: int | None = None
        try:
            if time.time() >= deadline:
                raise RuntimeError("controlled validation budget of 204 minutes has elapsed")
            run_id = dispatch(args.repository, ref, target_sha)
            records.append(
                collect_case(
                    args,
                    index,
                    phase,
                    target_sha,
                    ref,
                    expected_shards,
                    expected_cache,
                    run_id,
                    deadline,
                )
            )
        except Exception as error:
            (args.output_dir / "failure.json").write_text(
                json.dumps(
                    {
                        "failed_index": index,
                        "phase": phase,
                        "target_sha": target_sha,
                        "run_id": run_id,
                        "error": str(error),
                    },
                    sort_keys=True,
                )
                + "\n"
            )
            print(f"validation stopped at sample {index}/{TOTAL_DISPATCHES}: {error}", file=sys.stderr)
            return False
    return True


def collect_case(
    args: argparse.Namespace,
    index: int,
    phase: str,
    target_sha: str,
    ref: str,
    expected_shards: int,
    expected_cache: str | None,
    run_id: int,
    deadline: float,
    historical_run: bool = False,
) -> dict[str, Any]:
    run_dir = args.output_dir / f"{index:02d}-{phase}"
    watch(
        args.repository,
        run_id,
        args.timeout_seconds,
        args.interval_seconds,
        matrix_deadline=deadline,
        allow_completed_after_deadline=historical_run,
    )
    payload = download_metrics(args.repository, run_id, run_dir)
    if time.time() >= deadline:
        raise RuntimeError("controlled validation budget of 204 minutes has elapsed")
    validate_sample(payload, target_sha, expected_cache)
    if payload["storybook_shard_total"] != expected_shards:
        raise ValueError(f"expected {expected_shards} Storybook shards, got {payload['storybook_shard_total']}")
    return {
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
        "created_at": payload["created_at"],
    }


def write_matrix(
    output_dir: Path,
    phase: str,
    records: list[dict[str, Any]],
    selection: dict[str, Any] | None,
) -> None:
    output_dir.joinpath("matrix.json").write_text(
        json.dumps(
            {
                "phase": phase,
                "dispatch_count": len(records),
                "candidate_dispatch_count": sum(
                    record.get("phase") in {"two-shard", "three-shard"} for record in records
                ),
                "final_dispatch_count": sum(
                    record.get("phase") in {"cold-warmup", "warm"} for record in records
                ),
                "selection": selection,
                "records": records,
            },
            sort_keys=True,
        )
        + "\n"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--phase", choices=("all", "candidates", "final"), default="all")
    parser.add_argument("--two-shard-sha")
    parser.add_argument("--two-shard-ref")
    parser.add_argument("--three-shard-sha")
    parser.add_argument("--three-shard-ref")
    parser.add_argument("--final-sha")
    parser.add_argument("--final-ref")
    parser.add_argument("--candidate-dir", type=Path)
    parser.add_argument("--resume-run-id", type=int, action="append", default=[])
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--timeout-seconds", type=int, default=720)
    parser.add_argument("--interval-seconds", type=int, default=15)
    args = parser.parse_args()

    if args.timeout_seconds != 720 or args.interval_seconds != 15:
        parser.error("the acceptance contract fixes timeout-seconds=720 and interval-seconds=15")
    if args.output_dir.exists():
        parser.error("output-dir must not already exist; use a new directory for each matrix")
    if args.phase == "final" and args.candidate_dir is None:
        parser.error("final phase requires --candidate-dir from the completed candidates phase")
    if args.phase != "final" and args.candidate_dir is not None:
        parser.error("--candidate-dir is only valid for the final phase")
    if args.resume_run_id and args.phase != "final":
        parser.error("--resume-run-id is only valid for the final phase")
    try:
        validate_resume_run_ids(args.resume_run_id)
    except ValueError as error:
        parser.error(str(error))

    needs_candidates = args.phase in {"all", "candidates"}
    needs_final = args.phase in {"all", "final"}
    for name in ("two_shard_sha", "two_shard_ref", "three_shard_sha", "three_shard_ref"):
        if needs_candidates and not getattr(args, name):
            parser.error(f"{name.replace('_', '-')} is required for {args.phase} phase")
    for name in ("final_sha", "final_ref"):
        if needs_final and not getattr(args, name):
            parser.error(f"{name.replace('_', '-')} is required for {args.phase} phase")
    for name in ("two_shard_sha", "three_shard_sha", "final_sha"):
        value = getattr(args, name)
        if value:
            setattr(args, name, validate_sha(value, name))
    args.output_dir.mkdir(parents=True)

    records: list[dict[str, Any]] = []
    final_records: list[dict[str, Any]] = []
    selection: dict[str, Any] | None = None

    try:
        if args.phase == "final":
            candidate_records, selection, deadline = read_candidate_matrix(args.candidate_dir)
            records.extend(candidate_records)
            if args.final_sha != selection["selected_sha"] or args.final_ref != selection["selected_ref"]:
                raise ValueError("final target must exactly match the matrix selected from candidate P90s")
            shutil.copy2(args.candidate_dir / "deadline.json", args.output_dir / "deadline.json")
        else:
            deadline = write_deadline(args.output_dir / "deadline.json")
            candidate_cases = build_cases(args, "candidates")
            if not run_cases(args, candidate_cases, 1, deadline, records):
                return 1
            selection = select_final_matrix(records)
            write_matrix(args.output_dir, "candidates", records, selection)
            if args.phase == "candidates":
                print(json.dumps(selection, sort_keys=True) + "\n", end="")
                return 0

        final_cases = build_cases(args, "final", selection["selected_shards"])
        final_start = len(records) + 1
        for resume_run_id in args.resume_run_id:
            recovered_case = final_cases.pop(0)
            phase, target_sha, ref, expected_shards, expected_cache = recovered_case
            try:
                final_records.append(
                    collect_case(
                        args,
                        final_start,
                        phase,
                        target_sha,
                        ref,
                        expected_shards,
                        expected_cache,
                        resume_run_id,
                        deadline,
                        historical_run=True,
                    )
                )
            except Exception as error:
                (args.output_dir / "failure.json").write_text(
                    json.dumps(
                        {
                            "failed_index": final_start,
                            "phase": phase,
                            "target_sha": target_sha,
                            "run_id": resume_run_id,
                            "error": str(error),
                        },
                        sort_keys=True,
                    )
                    + "\n"
                )
                print(
                    f"validation stopped at resumed sample {final_start}/{TOTAL_DISPATCHES}: {error}",
                    file=sys.stderr,
                )
                return 1
            if len(final_records) > 1:
                previous_created = parse_utc(final_records[-2]["created_at"], "created_at")
                current_created = parse_utc(final_records[-1]["created_at"], "created_at")
                if current_created <= previous_created:
                    raise ValueError("resumed GitHub runs must be supplied in chronological order")
            final_start += 1
        if not run_cases(args, final_cases, final_start, deadline, final_records):
            return 1
        records.extend(final_records)
        write_matrix(args.output_dir, args.phase, records, selection)
    except Exception as error:
        (args.output_dir / "failure.json").write_text(json.dumps({"error": str(error)}, sort_keys=True) + "\n")
        print(f"validation stopped before dispatch: {error}", file=sys.stderr)
        return 1

    warm_dir = args.output_dir / "warm-metrics"
    warm_dir.mkdir()
    for record in final_records[-10:]:
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

    print(verifier.stdout, end="")
    print(f"controlled validation passed: {len(records)} serial runs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
