# Dockrev Release Channel Promotion 主题历史

## Lifecycle / Compatibility

- This active topic refines the channel boundary of ADR 0008 without changing
  its PR-local preparation, tag reservation, or same-SHA recovery model.

## Replacements / Background

- `mzqkx-release-channel-selection` is retained as historical stable/RC-only
  material. The current four-channel and promotion contract is defined here and
  by ADR 0009.

- Automatic version allocation follows ADR 0010: the highest qualified final
  release is the numeric baseline, while source `VERSION` remains an
  identity/provenance input. Legacy no-prefix tags are accepted as frozen
  historical baselines, canonical `vX.Y.Z` tags remain the future publication
  format, and conflicting qualified targets fail closed. The baseline is frozen
  in signed release provenance and revalidated at each release identity
  boundary.

- Successor ADR 0011 defines the PR #391 historical backfill boundary. ADR
  0012 adds the independently enumerated PR #395 boundary:
  `ff1b57b6835616cd3b7a95a479c0106426eb5d40` receives the `0.80.3`
  version-only identity with baseline `0.80.2` and stable patch intent.
  PR #390 remains part of the PR #391 boundary and is not independently
  released.

- Recovery preparation is selected by the `recovery/` branch prefix. Automatic
  workflow-run preparation skips that prefix, and the version-only helper
  requires it before creating an identity. Annotated tag traversal now matches
  the publication workflow's five-hop boundary.

- Version-only retries reuse the existing signed single-parent identity and its
  source-parent checks. Label Gate evidence is restricted to the trusted
  `pull_request_target` event, and final identity resolution rejects
  multi-parent recovery identities.

- Canonical version reservations now point directly to the verified signed
  identity commit. The resolver keeps legacy reservation commits readable as
  historical evidence, but no new unsigned reservation commit is created.

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
