#!/usr/bin/env python3
"""Fast local contract tests for CI scope, source gates, and timing metrics."""

from __future__ import annotations

import importlib.util
import json
import tempfile
from datetime import datetime, timedelta, timezone
from pathlib import Path


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
        "final_shards": 3,
    },
)()
cases = validation_runner.build_cases(runner_args)
assert_equal(len(cases), 17)
assert_equal(cases[0][0:4], ("two-shard", "a" * 40, "candidate-two", 2))
assert_equal(cases[5][0:4], ("three-shard", "b" * 40, "candidate-three", 3))
assert_equal(cases[6][0:5], ("cold-warmup", "c" * 40, "candidate-final", 3, "cold"))
assert_equal(cases[-1][0:5], ("warm", "c" * 40, "candidate-final", 3, "warm"))
original_which = validation_runner.shutil.which
original_invoke = validation_runner.invoke
watch_call = {}
validation_runner.shutil.which = lambda _: None

def fake_invoke(command, *, capture=False, timeout_seconds=None):
    watch_call.update(command=command, capture=capture, timeout_seconds=timeout_seconds)
    return validation_runner.subprocess.CompletedProcess(command, 0, "", "")

validation_runner.invoke = fake_invoke
validation_runner.watch("acme/dockrev", 123, 720, 15)
assert_equal(watch_call["command"][0:3], ["gh", "run", "watch"])
assert_equal(watch_call["capture"], True)
assert_equal(watch_call["timeout_seconds"], 720)
validation_runner.shutil.which = original_which
validation_runner.invoke = original_invoke

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
