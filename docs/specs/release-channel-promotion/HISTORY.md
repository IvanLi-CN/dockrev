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

- Successor ADR 0011 defines the one historical backfill boundary: PR #391
  merge `978207fe9d140d81e2d4a2a7bd24fb253a04ebff` receives the sole
  `0.80.2` version-only identity with baseline `0.80.1` and stable patch
  intent; PR #390 is not independently released.

- Recovery preparation is selected by the `recovery/` branch prefix. Automatic
  workflow-run preparation skips that prefix, and the version-only helper
  requires it before creating an identity. Annotated tag traversal now matches
  the publication workflow's five-hop boundary.

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
