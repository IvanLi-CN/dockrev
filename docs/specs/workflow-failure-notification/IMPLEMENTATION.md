# Dockrev：预期成功工作流失败通知实现

## Current Status

- Implementation: complete in the checked-in workflow, policy, helper, contract test, and documentation.
- Lifecycle: active.
- External delivery: Oidrune/OIDC caller contract is reused; gateway and OIDC allowlist changes remain owner actions outside this repository.

## Implementation Coverage

- `REQ-WORKFLOW-FAILURE-001`: `.github/release-failure-notification.json` declares the seven expected-success workflows and two notification exclusions. `.github/scripts/test_workflow_failure_notification_contract.py` compares those sets with every workflow's top-level name.
- `REQ-WORKFLOW-FAILURE-002`: `.github/workflows/notify-failed-workflow.yml` listens to `CI (PR)`, `Docs Pages`, `Label Gate`, `Release completion`, `Release Preparation`, and `Review Policy`; `Release` remains exclusively in `.github/workflows/notify-release-failure.yml`.
- `REQ-WORKFLOW-FAILURE-003`: `.github/scripts/workflow_failure_context.py` validates failure/run identifiers and renders the workflow metadata, head SHA, PR, attempt, actor, run URL, and recovery guidance without release identity claims.
- `REQ-WORKFLOW-FAILURE-004`: the generic notifier checks out only `github.event.repository.default_branch`, uses the same 40-character pinned Oidrune ref for each delivery path, grants only `contents: read` to context resolution and `id-token: write` to delivery, and does not execute the failed caller ref. The contract test rejects truncated or divergent reusable-workflow refs before publication.

## Implementation Order

1. Extend the existing notification transport gate with expected-success workflow classification and explicit routes.
2. Add the generic trusted context renderer and its behavioral fixture.
3. Add the `workflow_run` sidecar with a manual smoke path.
4. Add the contract test to `CI (PR)` and update the release/Pages documentation boundaries.

## Runtime Behavior

```text
expected workflow completed
        |
        +-- conclusion != failure -> no notification
        |
        +-- Release -> Notify failed release -> identity-aware summary
        |
        +-- other expected workflow -> Notify failed workflow -> generic summary
```

The generic route is intentionally metadata-only. It does not download artifacts, resolve a version, or retry a release. This keeps a failed Pages/CI gate actionable without confusing it with a failed publication identity.

## Validation Evidence

- `test_workflow_failure_notification_contract.py`: passed locally.
- `test_docs_pages_workflow_contract.py`: passed locally.
- `release-channel-contract-check.sh`: passed locally, including existing release fixtures and workflow contract checks.
- `actionlint .github/workflows/*.yml`: passed locally.
- `git diff --check`: passed locally.

## Remaining Gaps

- No real Oidrune notification is sent during local validation; the manual smoke path remains the owner-operated live check.
- External OIDC subject/audience allowlist and Oidrune delivery configuration are not repository writes.
