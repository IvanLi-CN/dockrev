#!/usr/bin/env python3
"""Fast local contract tests for CI scope, source gates, and timing metrics."""

from __future__ import annotations

import importlib.util
import json
import tempfile
from datetime import datetime, timedelta, timezone
from email.message import Message
from pathlib import Path
from urllib.request import Request


ROOT = Path(__file__).resolve().parents[2]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


scope = load("scope", ROOT / ".github/scripts/resolve-ci-scope.py")
gate = load("gate", ROOT / ".github/scripts/release_source_gate.py")
metrics = load("metrics", ROOT / ".github/scripts/verify_ci_gate_metrics.py")
storybook_matrix = load("storybook_matrix", ROOT / ".github/scripts/resolve-storybook-matrix.py")
validation_runner = load("validation_runner", ROOT / ".github/scripts/run_ci_gate_validation.py")


def assert_equal(actual, expected):
    if actual != expected:
        raise AssertionError(f"expected {expected!r}, got {actual!r}")


assert_equal(scope.matches("web/src/App.tsx", scope.WEB_PATTERNS), True)
assert_equal(scope.matches("crates/api/src/lib.rs", scope.DOCKER_PATTERNS), True)
assert_equal(scope.matches("docs/README.md", scope.DOCKER_PATTERNS), False)
assert_equal(scope.resolve("base", "head", True)[:2], (True, True))
matrix = storybook_matrix.matrix()
assert_equal(len(matrix), 4)
assert_equal(matrix[0]["role"], "global")
assert_equal(matrix[-1]["shard"], "3/3")
runner_args = type(
    "RunnerArgs",
    (),
    {
        "two_shard_sha": "a" * 40,
        "two_shard_ref": "candidate-two",
        "three_shard_sha": "b" * 40,
        "three_shard_ref": "candidate-three",
        "final_sha": "c" * 40,
        "final_ref": "candidate-final",
    },
)()
candidate_cases = validation_runner.build_cases(runner_args, "candidates")
assert_equal(len(candidate_cases), 6)
assert_equal(candidate_cases[0][0:4], ("two-shard", "a" * 40, "candidate-two", 2))
assert_equal(candidate_cases[5][0:4], ("three-shard", "b" * 40, "candidate-three", 3))
final_cases = validation_runner.build_cases(runner_args, "final", 3)
assert_equal(len(final_cases), 10)
assert_equal(final_cases[0][0:5], ("warm", "c" * 40, "candidate-final", 3, "warm"))
assert_equal(final_cases[-1][0:5], ("warm", "c" * 40, "candidate-final", 3, "warm"))
all_cases = validation_runner.build_cases(runner_args, "all", 3)
assert_equal(len(all_cases), 16)
validation_runner.validate_resume_run_ids([1, 2, 3])
for invalid_resume_ids, expected_error in (([0], "positive"), ([1, 1], "unique"), (list(range(11)), "at most")):
    try:
        validation_runner.validate_resume_run_ids(invalid_resume_ids)
    except ValueError as error:
        assert_equal(expected_error in str(error), True)
    else:
        raise AssertionError("invalid resumed run ids must fail")

candidate_records = [
    {"phase": "two-shard", "target_sha": "a" * 40, "ref": "candidate-two", "fast_seconds": value}
    for value in (300, 305, 310)
] + [
    {"phase": "three-shard", "target_sha": "b" * 40, "ref": "candidate-three", "fast_seconds": value}
    for value in (311, 315, 320)
]
selection = validation_runner.select_final_matrix(candidate_records)
assert_equal(selection["selected_shards"], 3)
assert_equal(selection["reason"], "fast P90 difference below 30 seconds; preserve three-shard coverage")
faster_two = [dict(record) for record in candidate_records]
for record in faster_two:
    if record["phase"] == "three-shard":
        record["fast_seconds"] += 100
assert_equal(validation_runner.select_final_matrix(faster_two)["selected_shards"], 2)
warm_payload = {
    "target_sha": "a" * 40,
    "fast_target_sha": "a" * 40,
    "storybook_shard_total": 3,
    "storybook_story_count": 381,
    "coverage_summary": "b" * 64,
    "scope": "full",
    "web": True,
    "docker": True,
    "publish": False,
    "cache_status": "cold",
    "fast_result": "success",
    "source_result": "success",
    "coverage_result": "success",
    "verification_mode": True,
    "created_at": "2026-09-01T00:00:00Z",
    "run_started_at": "2026-09-01T00:00:00Z",
    "fast_completed_at": "2026-09-01T00:05:29Z",
    "source_completed_at": "2026-09-01T00:10:48Z",
    "eligibility_completed_at": "2026-09-01T00:10:48Z",
    "fast_seconds": 329,
    "source_seconds": 648,
    "eligibility_seconds": 648,
    "execution_seconds": 648,
    "queue_seconds": 0,
}
validation_runner.validate_sample(warm_payload, "a" * 40, None)
try:
    validation_runner.validate_sample(dict(warm_payload, cache_status="warm"), "a" * 40, "warm")
except ValueError as error:
    assert_equal("600 second investigation threshold" in str(error), True)
else:
    raise AssertionError("warm samples above 600 seconds must fail")
with tempfile.TemporaryDirectory() as directory:
    candidate_dir = Path(directory)
    validation_runner.write_deadline(candidate_dir / "deadline.json")
    validation_runner.write_matrix(candidate_dir, "candidates", candidate_records, selection)
    loaded_records, loaded_selection, deadline = validation_runner.read_candidate_matrix(candidate_dir)
    assert_equal(len(loaded_records), 6)
    assert_equal(loaded_selection["selected_shards"], 3)
    assert_equal(deadline > 0, True)
original_invoke = validation_runner.invoke
original_sleep = validation_runner.time.sleep
original_time = validation_runner.time.time
watch_calls = []
clock = [0.0]
responses = iter(
    [
        '{"status":"queued","conclusion":null,"startedAt":null}',
        '{"status":"in_progress","conclusion":null,"startedAt":"1970-01-01T00:00:00Z"}',
        '{"status":"completed","conclusion":"success","startedAt":"1970-01-01T00:00:00Z"}',
    ]
)

def fake_invoke(command, *, capture=False, timeout_seconds=None):
    watch_calls.append((command, capture, timeout_seconds))
    return validation_runner.subprocess.CompletedProcess(command, 0, next(responses), "")

validation_runner.time.time = lambda: clock[0]
validation_runner.time.sleep = lambda seconds: clock.__setitem__(0, clock[0] + seconds)
validation_runner.invoke = fake_invoke
validation_runner.watch("acme/dockrev", 123, 720, 15)
assert_equal(len(watch_calls), 3)
assert_equal(watch_calls[0][0][0:3], ["gh", "run", "view"])
assert_equal(watch_calls[0][1:], (True, 60))
assert_equal(clock[0], 30)

historical_calls = []
validation_runner.time.time = lambda: 2_000.0
validation_runner.invoke = lambda command, *, capture=False, timeout_seconds=None: (
    historical_calls.append((command, capture, timeout_seconds))
    or validation_runner.subprocess.CompletedProcess(
        command,
        0,
        '{"status":"completed","conclusion":"success","startedAt":"1970-01-01T00:00:00Z"}',
        "",
    )
)
validation_runner.watch("acme/dockrev", 456, 720, 15, allow_completed_after_deadline=True)
assert_equal(len(historical_calls), 1)

retry_calls = []
retry_clock = [0.0]
retry_responses = iter(
    [
        validation_runner.subprocess.CompletedProcess([], 1, "", "redacted"),
        validation_runner.subprocess.CompletedProcess(
            [],
            0,
            '{"status":"completed","conclusion":"success","startedAt":"1970-01-01T00:00:00Z"}',
            "",
        ),
    ]
)


def fake_retry_invoke(command, *, capture=False, timeout_seconds=None):
    retry_calls.append((command, capture, timeout_seconds))
    return next(retry_responses)


validation_runner.time.time = lambda: retry_clock[0]
validation_runner.time.sleep = lambda seconds: retry_clock.__setitem__(0, retry_clock[0] + seconds)
validation_runner.invoke = fake_retry_invoke
validation_runner.watch("acme/dockrev", 127, 720, 15)
assert_equal(len(retry_calls), 2)
assert_equal(retry_calls[0][1:], (True, 60))
assert_equal(retry_clock[0], validation_runner.STATUS_QUERY_RETRY_SECONDS)

transport_calls = []
transport_clock = [0.0]
transport_responses = iter(
    [
        validation_runner.subprocess.CompletedProcess([], 1, "", "redacted"),
        validation_runner.subprocess.CompletedProcess([], 1, "", "redacted"),
        validation_runner.subprocess.CompletedProcess([], 1, "", "redacted"),
        validation_runner.subprocess.CompletedProcess(
            [],
            0,
            '{"status":"completed","conclusion":"success","startedAt":"1970-01-01T00:00:00Z"}',
            "",
        ),
    ]
)


def fake_transport_invoke(command, *, capture=False, timeout_seconds=None):
    transport_calls.append((command, capture, timeout_seconds))
    return next(transport_responses)


validation_runner.time.time = lambda: transport_clock[0]
validation_runner.time.sleep = lambda seconds: transport_clock.__setitem__(
    0, transport_clock[0] + seconds
)
validation_runner.invoke = fake_transport_invoke
validation_runner.watch("acme/dockrev", 128, 720, 15)
assert_equal(len(transport_calls), 4)
assert_equal(
    transport_clock[0], validation_runner.STATUS_QUERY_RETRY_SECONDS * 2 + 15
)

deadline_clock = [901.0]
deadline_responses = iter(
    ['{"status":"completed","conclusion":"success","startedAt":"1970-01-01T00:00:00Z"}']
)

def fake_deadline_invoke(command, *, capture=False, timeout_seconds=None):
    return validation_runner.subprocess.CompletedProcess(command, 0, next(deadline_responses), "")

validation_runner.time.time = lambda: deadline_clock[0]
validation_runner.invoke = fake_deadline_invoke
try:
    validation_runner.watch("acme/dockrev", 124, 720, 15)
except RuntimeError as error:
    assert_equal("observation grace" in str(error), True)
else:
    raise AssertionError("watch must reject success observed after the fixed observation deadline")

queue_clock = [0.0]
queue_calls = []
queue_responses = iter(
    [
        '{"status":"queued","conclusion":null,"startedAt":null}',
        '{"status":"in_progress","conclusion":null,"startedAt":"1970-01-01T02:46:40Z"}',
        '{"status":"completed","conclusion":"success","startedAt":"1970-01-01T02:46:40Z"}',
    ]
)

def fake_queue_invoke(command, *, capture=False, timeout_seconds=None):
    queue_calls.append((command, capture, timeout_seconds))
    if len(queue_calls) == 1:
        queue_clock[0] = 10_000.0
    return validation_runner.subprocess.CompletedProcess(command, 0, next(queue_responses), "")

validation_runner.time.time = lambda: queue_clock[0]
validation_runner.time.sleep = lambda seconds: queue_clock.__setitem__(0, queue_clock[0] + seconds)
validation_runner.invoke = fake_queue_invoke
validation_runner.watch("acme/dockrev", 126, 720, 15)
assert_equal(len(queue_calls), 3)
assert_equal(queue_calls[0][1:], (True, 60))
assert_equal(queue_clock[0], 10_030.0)

validation_runner.time.sleep = original_sleep
validation_runner.time.time = original_time
validation_runner.invoke = original_invoke

failure_responses = iter(
    ['{"status":"completed","conclusion":"failure","startedAt":"1970-01-01T00:00:00Z"}']
)

def fake_failed_invoke(command, *, capture=False, timeout_seconds=None):
    return validation_runner.subprocess.CompletedProcess(command, 0, next(failure_responses), "")

validation_runner.invoke = fake_failed_invoke
validation_runner.time.time = lambda: 0.0
try:
    validation_runner.watch("acme/dockrev", 125, 720, 15)
except RuntimeError as error:
    assert_equal("conclusion 'failure'" in str(error), True)
else:
    raise AssertionError("watch must fail closed for a non-success terminal conclusion")
validation_runner.invoke = original_invoke
validation_runner.time.time = original_time

target_sha = "a" * 40
valid_attestation = {
    "target_sha": target_sha,
    "gate": "source-build-release",
    "scope": "full",
    "docker": True,
    "web": True,
    "source_result": "success",
    "publish": False,
}
assert_equal(gate.validate_attestation(valid_attestation, target_sha), (True, "ok"))
invalid_attestation = dict(valid_attestation, publish=True)
assert_equal(gate.validate_attestation(invalid_attestation, target_sha)[0], False)
valid_push_attestation = {
    "target_sha": target_sha,
    "gate": "source-build-release",
    "scope": "changed",
    "web": True,
    "docker": True,
    "source_result": "success",
    "verification_mode": False,
    "publish": False,
}
assert_equal(gate.validate_push_attestation(valid_push_attestation, target_sha), (True, "ok"))
assert_equal(
    gate.validate_push_attestation(dict(valid_push_attestation, source_result="skipped"), target_sha)[0],
    False,
)
assert_equal(
    gate.validate_push_attestation(
        dict(valid_push_attestation, docker=False, source_result="skipped"), target_sha
    ),
    (True, "ok"),
)

artifact_request = Request(
    "https://api.github.com/repos/acme/dockrev/actions/artifacts/1/zip",
    headers={
        "Accept": "application/vnd.github+json",
        "Authorization": "Bearer redacted",
        "X-GitHub-Api-Version": "2022-11-28",
    },
)
redirect_handler = gate.ArtifactRedirectHandler()
artifact_redirect = redirect_handler.redirect_request(
    artifact_request,
    None,
    302,
    "Found",
    Message(),
    "https://pipelines.actions.githubusercontent.com/signed-artifact",
)
assert_equal(artifact_redirect is not None, True)
assert_equal(artifact_redirect.get_header("Authorization"), None)
assert_equal(artifact_redirect.get_header("X-github-api-version"), None)
assert_equal(artifact_redirect.get_header("Accept"), "application/octet-stream")
same_origin_redirect = redirect_handler.redirect_request(
    artifact_request,
    None,
    302,
    "Found",
    Message(),
    "https://api.github.com/repos/acme/dockrev/actions/artifacts/1/zip?retry=1",
)
assert_equal(same_origin_redirect is not None, True)
assert_equal(same_origin_redirect.get_header("Authorization"), "Bearer redacted")

source_run = {
    "event": "push",
    "head_branch": "main",
    "head_sha": target_sha,
    "status": "completed",
    "conclusion": "success",
    "id": 11,
}
fast_run = dict(source_run, id=12)
original_api_json = gate.api_json
original_download_attestation = gate.download_attestation
gate.api_json = lambda url, token: {
    "workflow_runs": [source_run if "source-build-release-gate" in url else fast_run]
}
gate.download_attestation = lambda api_root, repository, run_id, token: valid_push_attestation
assert_equal(
    gate.wait_for_push_gate(
        api_root="https://api.invalid",
        repository="acme/dockrev",
        target_sha=target_sha,
        token="redacted",
        timeout_seconds=1,
        poll_seconds=1,
    )["target_sha"],
    target_sha,
)
gate.download_attestation = lambda api_root, repository, run_id, token: dict(valid_push_attestation, publish=True)
try:
    gate.wait_for_push_gate(
        api_root="https://api.invalid",
        repository="acme/dockrev",
        target_sha=target_sha,
        token="redacted",
        timeout_seconds=1,
        poll_seconds=1,
    )
except RuntimeError as error:
    assert_equal("attestation rejected" in str(error), True)
else:
    raise AssertionError("invalid push attestation must fail closed")
finally:
    gate.api_json = original_api_json
    gate.download_attestation = original_download_attestation

failed_run = gate.find_failed_push_run(
    [{"event": "push", "head_branch": "main", "head_sha": target_sha, "status": "completed", "conclusion": "failure", "id": 7}],
    target_sha,
)
assert_equal(failed_run["id"], 7)
verification_metrics = {
    "target_sha": target_sha,
    "fast_target_sha": target_sha,
    "storybook_shard_total": 3,
    "storybook_story_count": 381,
    "coverage_summary": "b" * 64,
    "scope": "full",
    "web": True,
    "docker": True,
    "fast_result": "success",
    "source_result": "success",
    "coverage_result": "success",
    "verification_mode": True,
    "publish": False,
}
assert_equal(gate.validate_verification_metrics(verification_metrics, target_sha), (True, "ok"))
assert_equal(
    gate.validate_verification_metrics(dict(verification_metrics, coverage_result="failure"), target_sha)[0],
    False,
)

with tempfile.TemporaryDirectory() as directory:
    path = Path(directory)
    created = datetime(2026, 9, 1, tzinfo=timezone.utc)
    run_started = created + timedelta(seconds=1)
    for index in range(10):
        fast_completed = run_started + timedelta(seconds=300 + index)
        source_completed = run_started + timedelta(seconds=350 + index)
        eligibility_completed = max(fast_completed, source_completed)
        (path / f"{index:02d}.json").write_text(
            json.dumps(
                {
                    "target_sha": target_sha,
                    "fast_target_sha": target_sha,
                    "storybook_shard_total": 3,
                    "storybook_story_count": 381,
                    "coverage_summary": "b" * 64,
                    "scope": "full",
                    "web": True,
                    "docker": True,
                    "publish": False,
                    "cache_status": "warm",
                    "fast_result": "success",
                    "source_result": "success",
                    "coverage_result": "success",
                    "verification_mode": True,
                    "created_at": created.isoformat().replace("+00:00", "Z"),
                    "run_started_at": run_started.isoformat().replace("+00:00", "Z"),
                    "fast_completed_at": fast_completed.isoformat().replace("+00:00", "Z"),
                    "source_completed_at": source_completed.isoformat().replace("+00:00", "Z"),
                    "eligibility_completed_at": eligibility_completed.isoformat().replace("+00:00", "Z"),
                    "fast_seconds": 300 + index,
                    "source_seconds": 350 + index,
                    "eligibility_seconds": 350 + index,
                    "execution_seconds": 350 + index,
                    "queue_seconds": 1,
                }
            )
        )
    result = metrics.verify(metrics.load_metrics(path))
    assert_equal(result["fast_p90"], 308.0)

print("PASS: CI release gate contract fixtures")
