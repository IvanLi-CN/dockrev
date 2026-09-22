# Preserve Rollups After Legacy Raw Retirement

## Status

Accepted for implementation.

## Context

Resource metrics migrate from the primary SQLite database into a dedicated
metrics database. Legacy raw samples are subject to the normal raw retention
policy, while latest projections and long-window rollups outlive those raw
rows. A restart must not rebuild an outlived rollup from raw data that no
longer exists.

The previous startup guard treated any legacy source revision change after a
retention tombstone as unrecoverable, even when the metrics target had already
retired every legacy raw row and retained a verifiable rollup read model. The
same database also stores derived rollup row-count metadata and JSON
fingerprints; floating-point JSON formatting can differ without a numeric
value changing.

## Decision

When the active-service migration path proves that the metrics target has no
legacy raw rows left, it preserves existing long-window rollups after a legacy
source revision change. It still synchronizes the latest projection from the
available source and updates the migration manifest. If legacy raw rows remain,
the existing fail-closed guard stays in force.

Rollup integrity validation compares strings, integers, NULLs, and floating
point values according to their value types. Floating-point values use a
bounded relative tolerance to avoid treating serialization precision as data
corruption. If the rollup content validates but the derived row-count metadata
is stale while its trusted and current counters agree, startup repairs that
metadata from the actual table count. A counter disagreement or content
mismatch remains untrusted and follows the existing recovery failure path.

## Consequences

- Retired legacy raw data is not resurrected merely to make a restart pass.
- Existing long-window rollups remain available when their retained content is
  verifiable.
- Genuine rollup loss or mutation still blocks startup instead of being marked
  trusted.
- Rollup startup validation performs a typed read and may repair only derived
  counter metadata; it does not rewrite rollup values.
