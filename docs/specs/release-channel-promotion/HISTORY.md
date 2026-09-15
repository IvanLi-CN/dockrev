# Dockrev Release Channel Promotion 主题历史

## Lifecycle / Compatibility

- This active topic refines the channel boundary of ADR 0008 without changing
  its PR-local preparation, tag reservation, or same-SHA recovery model.

## Replacements / Background

- `mzqkx-release-channel-selection` is retained as historical stable/RC-only
  material. The current four-channel and promotion contract is defined here and
  by ADR 0009.

- Automatic version allocation now follows ADR 0010: the highest published
  final release is the numeric baseline, while source `VERSION` remains
  identity/provenance input.

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
