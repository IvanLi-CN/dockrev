# Classify Overview Refreshes by Origin

The overview distinguishes initial snapshot loads, manual refreshes, event-driven refreshes, and recovery synchronization by their trigger rather than by whether data already exists. Manual refreshes retain the prior snapshot and show local feedback after 200ms; management invalidations and intact-session recovery remain silent so pushed invalidations cannot block the service list, while replay gaps, `resync_required`, and protocol-invalid events may request a full reconciliation. A universal five-second delay was rejected because it makes an explicit user action appear unresponsive, and a global immediate mask was rejected because background synchronization is not a user-visible loading state.

## Consequences

- Refresh coordination must carry an explicit origin and must not infer UI policy from the presence of cached data.
- Event-driven failures preserve the prior snapshot and expose a non-blocking stale/error signal; only an initial read without data uses the blocking error state.
- Tests and visual evidence must cover the four origins independently, including the 200ms manual threshold and the absence of an event-driven loading mask.
