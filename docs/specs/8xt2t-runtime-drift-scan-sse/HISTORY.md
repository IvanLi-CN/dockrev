# History

## 2026-02-17

- Implemented runtime diff scan, `/api/runtime-scans`, job events, and UI-triggered refresh behavior.
- Established runtime scan as a lightweight reconciliation path rather than a full slow check replacement.

## 2026-06-09

- Migrated the legacy plan into canonical `docs/specs/**` shape.
- Tightened the runtime truth-source contract for moving tags shared by multiple stacks: running container image ID / digest is authoritative for `current_digest`; host-local tag resolution is not authoritative for a still-running container.

## 2026-07-20

- Removed page-open runtime scan side effects from read-only UI surfaces (`overview`, `services`, `service detail`) after field evidence showed that mounting those pages could repeatedly enqueue `scope=all` runtime scans and drive unnecessary CPU usage.
- Clarified the contract: automatic runtime drift detection remains owned by the background scheduled scan, while explicit `/api/runtime-scans` stays available for operator-triggered reconciliation.
