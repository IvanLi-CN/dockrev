# Make Backup Cleanup Independent of Stack Runtime State

Backup cleanup is decided by the artifact's own retention eligibility: it must be successful, outside the retained backup set, and past its configured retention delay. It does not depend on the runtime state of unrelated services in the Stack, because intentionally stopped services are a normal operating state and must not leave expired artifacts indefinitely.

## Considered Options

- Require every Stack service to be running and healthy before deletion.
- Require only the backed-up service to be running and healthy.
- Use retention eligibility and managed-storage safety checks only.

## Consequences

- The current all-services health gate is removed from cleanup eligibility.
- Existing due artifacts are reconciled automatically after deployment, subject to `keepLast`.
- Each cleanup attempt records its time and any failure. A managed artifact found absent is recorded as verified missing, rather than as deleted by Dockrev.
- Canonical managed-path validation remains required before any deletion.
