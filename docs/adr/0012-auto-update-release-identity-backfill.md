# Auto-Update Release Identity Backfill

## Status

Accepted for implementation.

## Context

PR #395 (`ff1b57b6835616cd3b7a95a479c0106426eb5d40`) merged the auto-update
candidate settlement implementation with `type:none`, so it entered `main`
without an immutable release identity. The existing version-only preparation
contract intentionally accepted only the earlier PR #391 boundary and therefore
could not create a release identity for #395.

## Decision

Add one exact, immutable version-only boundary to the release policy:

- covered merge: `ff1b57b6835616cd3b7a95a479c0106426eb5d40` (PR #395);
- product version: `0.80.3`;
- frozen final baseline: `0.80.2`;
- intent: `type:patch channel:stable`.

The boundary is accepted only as this complete tuple. The trusted workflow must
create a signed, single-parent, `VERSION`-only identity on a `recovery/` PR;
the normal completion and merged identity checks remain authoritative. No
arbitrary historical merge, successor version, tag, or publication is enabled
by this exception.

## Consequences

- The missing release can be repaired through the existing auditable PR path.
- PR #395's release identity is distinct from the covered merge SHA: the
  recovery PR merge is the published identity and #395 is retained as its
  covered product boundary.
- Any future missing identity still requires an explicit policy change and a
  separately reviewed tuple; it cannot be inferred from merge order or labels.
