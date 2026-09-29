# Dockrev Expected-Success Workflow Failure Notification

## Context and Scope

`Notify failed release` handles identity-aware publication and same-identity
recovery failures. Other expected-success workflows need a generic notification
that carries run metadata without claiming a release identity. The expected
workflow set must follow the current delivery contract; Dockrev no longer has
a label gate.

In scope: expected-success classification, generic and Release-specific
routing, trusted failure context, notification sidecars, and their contract
tests. The existing Oidrune/OIDC delivery transport remains unchanged.

## Terms and Interfaces

- Expected-success workflow: a checked-in business or delivery workflow
  declared in `.github/release-failure-notification.json`.
- Release-specific route: `Notify failed release`, which validates immutable
  release identity, asset, and recovery fields.
- Generic route: `Notify failed workflow`, which renders trusted
  `workflow_run` metadata and does not infer release identity.
- Notification workflows are excluded from the expected-success set to
  prevent recursive notifications.

## Requirements

### REQ-WORKFLOW-FAILURE-001: Expected-success allowlist

The policy MUST list every current expected-success workflow and classify all
other workflow names as excluded. Contract tests MUST compare the union of
expected and excluded names with the top-level names in `.github/workflows/*.yml`.
An unclassified workflow MUST fail the contract.

### REQ-WORKFLOW-FAILURE-002: Failure routing

A failed `Release` run MUST use `Notify failed release` only. A failed
non-Release expected-success workflow MUST use `Notify failed workflow`.
Notification sidecars MUST be excluded so their own failures cannot recurse.
Removed workflows MUST be removed from both the expected-success policy and
the generic sidecar triggers.

### REQ-WORKFLOW-FAILURE-003: Generic failure context

Generic context MUST include repository, workflow, event, ref/branch, head SHA,
PR number when available, run ID, run attempt, actor, and run URL. It MUST NOT
invent version, tag, release identity, or same-identity recovery data for an
ordinary workflow failure.

### REQ-WORKFLOW-FAILURE-004: Trusted execution boundary

The generic sidecar MUST read its helper from the repository default branch and
MUST NOT checkout or execute failed caller code. Oidrune delivery MUST use the
selected pinned reusable workflow and OIDC contract. This repository MUST NOT
add notifier secrets or change external OIDC allowlists.

## Verification

### VER-WORKFLOW-FAILURE-001: Workflow classification

- Method: `python3 .github/scripts/test_workflow_failure_notification_contract.py`.
- covers: `REQ-WORKFLOW-FAILURE-001`, `REQ-WORKFLOW-FAILURE-002`.
- Pass condition: policy and workflow names match exactly; `Release` is routed
  only to its dedicated sidecar; removed label-gate workflows are absent.

### VER-WORKFLOW-FAILURE-002: Context and trusted delivery

- Method: workflow context fixtures, workflow contract checks, and `actionlint`.
- covers: `REQ-WORKFLOW-FAILURE-003`, `REQ-WORKFLOW-FAILURE-004`.
- Pass condition: generic context includes run metadata without release claims;
  helper checkout, OIDC caller pin, and permission boundaries remain valid.

### VER-WORKFLOW-FAILURE-003: Manual notifier smoke path

- Method: static contract for each notifier's `workflow_dispatch` path.
- covers: `REQ-WORKFLOW-FAILURE-002`, `REQ-WORKFLOW-FAILURE-004`.
- Pass condition: the smoke path calls only the notifier and does not start a
  business workflow or a live release.

## Related ADRs

None

## References

- [IMPLEMENTATION.md](./IMPLEMENTATION.md)
- [HISTORY.md](./HISTORY.md)
- `.github/release-failure-notification.json`
- `.github/workflows/notify-failed-workflow.yml`
- `.github/workflows/notify-release-failure.yml`
