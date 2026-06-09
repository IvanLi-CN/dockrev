# History

## 2026-02-17

- Implemented runtime diff scan, `/api/runtime-scans`, job events, and UI-triggered refresh behavior.
- Established runtime scan as a lightweight reconciliation path rather than a full slow check replacement.

## 2026-06-09

- Migrated the legacy plan into canonical `docs/specs/**` shape.
- Tightened the runtime truth-source contract for moving tags shared by multiple stacks: running container image ID / digest is authoritative for `current_digest`; host-local tag resolution is not authoritative for a still-running container.
