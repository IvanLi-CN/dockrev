# Prepare Exact-SHA Release Artifacts Before Publication

## Status

Superseded for orchestration by [ADR 0007](0007-event-driven-release-readiness.md); the artifact and manifest contract remains.

## Context

Release currently recompiles Web and four platform/libc binary variants after
the source-build gate has completed. The resulting images are assembled from
those binaries, but the compile work does not itself publish anything and can
be performed earlier for a release-enabled main commit.

## Decision

On each `main` push that has a release-enabled snapshot intent, a dedicated
`Release Preparation` workflow builds the Web distribution and all release
binary inputs for that exact commit. It uploads one immutable artifact with a
SHA-256 manifest and `publish=false`, using only read permissions and a one-day
retention period.

Release consumes the artifact selected by the snapshot queue's actual
`target_sha`, not the triggering workflow SHA. The artifact is now recorded in
the immutable readiness receipt described by ADR 0007; Release validates that
receipt and never owns preparation recovery.

The preparation artifact is an optimization and provenance input only. It
never replaces the independent Dockerfile source-build and Compose deployment
smoke release gate.

## Consequences

- Release no longer recompiles Web or release binaries on its critical path;
  it downloads, verifies, packages, and publishes the prepared inputs.
- A release-enabled main push uses additional parallel runner time and stores
  temporary artifacts for one day.
- Expiry or artifact-service loss blocks publication until an owner explicitly
  starts a new exact-SHA candidate pipeline.
- Removing this optimization is reversible by restoring the Release build jobs
  and removing the preparation dependency; source-build gating is unchanged.
