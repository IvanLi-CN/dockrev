# Event-Driven Release Candidate Readiness

## Status

Accepted for implementation.

## Context

The release workflow used to discover independently started fast CI, source
gate, and preparation runs after a snapshot became publishable. It had to poll
Actions APIs, wait for a shared timeout, and sometimes dispatch a recovery
preparation. A slow or missing artifact therefore made Release responsible for
orchestrating work it did not own, while a newer target could not provide a
durable proof for the older target.

## Decision

Keep the early parallel build, but make `Release Candidate Pipeline` the
owner of the exact-SHA candidate DAG. It calls fast CI, the source-build gate,
and release preparation as reusable workflows with the same target SHA. A
final candidate job validates the source attestation and preparation manifest,
then writes one immutable JSON note to `refs/notes/release-readiness`.

The receipt is bound to the target SHA, candidate run, source attestation run
and digest, preparation artifact run and manifest digest, and `publish=false`.
Release may publish only after validating the receipt and downloading exactly
the recorded artifacts. Automatic Release runs select the first-parent
oldest-ready pending snapshot and retain the `release-main` serialized queue.
There is no API polling or automatic recovery in Release. An operator may
start a new exact-SHA candidate explicitly after this change is merged.

`workflow_dispatch` verification mode is non-publishing. It can run the three
reusable gates and create validation artifacts, but it cannot write a
readiness note, create a tag, push GHCR, or create a GitHub Release.

## Consequences

- Candidate preparation remains parallel with fast and source checks, so the
  publication critical path still consumes prepared files.
- Release becomes a proof consumer with one bounded download/validation pass;
  a missing or mismatched proof fails closed without a recovery side effect.
- The Actions run id is part of the receipt, so artifact ownership is explicit
  even when reusable workflows are moved or their artifact retention changes.
- The existing snapshot, publication ledger, tag, GHCR, and Release naming
  contracts remain unchanged.
