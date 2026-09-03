# l2nm4 Release failure Telegram alerts history

## Lifecycle

- The notification boundary moved from the shared Telegram reusable workflow
  to the Oidrune reusable workflow.

## Compatibility

- The dockrev wrapper retains the `workflow_run` failure filter, release target
  SHA resolution and fallback, and no-input `workflow_dispatch` smoke path.
- The caller-owned summary preserves the previous notification context so the
  downstream notifier does not need to infer dockrev metadata.
