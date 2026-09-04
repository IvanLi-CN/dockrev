# Dockrev CI Duration Optimization and Release Assurance 主题历史

## Lifecycle / Compatibility

- New topic. It preserves the existing release snapshot queue, `head_sha`
  dispatch input, required-check names, and local Compose source-build behavior.

## Replacements / Background

- The historical `docs/plan/0005:github-actions-performance/` material is
  background only; this slug-only topic owns the current source-gate contract.
- ADR `0005-source-build-release-gate.md` records why source-built smoke remains
  release-blocking while moving off the fast critical path.
- ADR `0006-early-release-preparation-artifacts.md` records why exact-SHA
  preparation artifacts may accelerate Release without becoming source-build
  proof.

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
