# Release Version Baseline

## Status

Accepted for implementation.

## Context

The PR-local release preparation flow currently reads `VERSION` from the
source SHA and uses that value as the automatic stable patch base. `VERSION` is
the identity written by preparation, so using it as a counter can make a stale
or manually bootstrapped source select a successor unrelated to the latest
published product release.

The Style Playbook `PR label release` contract defines the highest qualified
final `vX.Y.Z` release as the only numeric baseline and uses `0.0.0` when no
final release exists. `VERSION` remains necessary provenance, but is not a
fallback allocator.

## Decision

`Release Preparation` reads published, non-draft, non-prerelease GitHub
releases whose tags are final semver `vX.Y.Z` values, selects the greatest
numeric version, and uses it as the allocation baseline. If no qualifying
release exists, the baseline is `0.0.0`. The source commit's `VERSION` is still
read and validated for source identity and channel-promotion lineage.

The automatic stable patch path allocates the next patch after the final
baseline. Explicit major, minor, beta, RC, dev, and prerelease-to-stable
versions remain caller inputs and are validated against the same baseline and
source lineage before reservation. No package manifest, source `VERSION`, PR
ordering, or recovery state can replace the final-release baseline.

## Consequences

- A stale source `VERSION` cannot advance a release beyond the actual published
  final line.
- Repositories with no published final release deterministically start at
  `0.0.0`.
- Release preparation now depends on authenticated GitHub release metadata; API
  errors fail closed instead of silently falling back to `VERSION`.
