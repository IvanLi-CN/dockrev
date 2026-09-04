# Prepare Exact-SHA Release Artifacts Before Publication

## Status

Accepted for implementation as part of CI duration optimization.

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
`target_sha`, not the triggering workflow SHA. The Release evaluator accepts
only a successful trusted-main preparation run, an exact target SHA, the
complete manifest, and the non-publishing marker. If the artifact is missing
or expired, Release emits a warning and dispatches at most one target-bound
recovery preparation. A failed, timed-out, malformed, or mismatched recovery
blocks publication.

The preparation artifact is an optimization and provenance input only. It
never replaces the independent Dockerfile source-build and Compose deployment
smoke release gate.

## Consequences

- Release no longer recompiles Web or release binaries on its critical path;
  it downloads, verifies, packages, and publishes the prepared inputs.
- A release-enabled main push uses additional parallel runner time and stores
  temporary artifacts for one day.
- Expiry or artifact-service loss can add one bounded recovery build, with a
  visible warning and no publication bypass.
- Removing this optimization is reversible by restoring the Release build jobs
  and removing the preparation dependency; source-build gating is unchanged.
