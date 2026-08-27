# Record Lifecycle Transitions As Durable Service Events

Dockrev records 30 days of lifecycle transitions in a dedicated service-event ledger instead of deriving them from resource-sampling gaps or Job log text. An operation-scoped Docker Engine event observer captures stop boundaries, while final container inspection supplies `StartedAt` and runtime-state confirmation without changing existing Compose command semantics; resource-chart annotations and typed service-log rows are projections of that ledger.

## Considered Options

- Derive transitions from resource history or Job logs: rejected because sampling gaps and free-form log lines do not prove a service state change.
- Split `compose restart` into separate stop and start commands: rejected because it changes the established restart behavior.

## Consequences

- Every Dockrev-managed path that changes a service lifecycle must identify its affected services and emit correlated lifecycle events.
- Stack operations create per-service events related by one lifecycle operation group.
- A missing observed boundary keeps any confirmed transition visible but never creates a fabricated availability interval.
- Lifecycle events persist independently of resource-monitoring enablement.
