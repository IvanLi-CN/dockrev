# Keep Source-Built Deployment Smoke Release-Blocking

## Status

Accepted for implementation on the CI duration optimization branch.

## Context

The main CI run currently combines fast checks with a Dockerfile source build and
Compose deployment smoke. The source build is the dominant cost, but it is also
the only check that proves the shipped Docker runtime can be built from the
target source. Removing it from the release path would weaken publication
safety.

## Decision

Split fast `CI (main)` feedback from a separate `Source Build Release Gate`.
The source gate keeps the ordinary `runtime` and `runtime-supervisor` Dockerfile
targets, runs the authored Compose smoke topology, and produces an exact-SHA
attestation. Release publication must have successful fast and source gates for
the same target SHA. A failed, missing, or unverifiable source gate blocks all
release build and publish jobs.

The source gate may use Buildx GitHub Actions cache scopes to reduce repeated
Cargo compilation. Cache failures never bypass source-build or Compose checks.
The existing `runtime-prebuilt` targets remain for artifact-first release jobs;
they are not evidence for this source-build gate.

## Considered Options

- Keep the source-built deployment smoke inside the fast main gate: rejected
  because its distinct cost and safety role cannot be measured or optimized
  independently.
- Make the source-built deployment smoke post-release or scheduled-only:
  rejected because a Dockerfile or source-build regression could reach a
  published release.
- Use only the prebuilt deployment smoke as the publication gate: rejected
  because it does not prove the Dockerfile can build the shipped binaries from
  the target source.

## Consequences

- Fast CI can finish without waiting for the source build, while publication
  remains blocked until the exact-SHA source gate succeeds.
- The local source-build Compose configuration remains usable; CI-specific
  loaded-image wiring is opt-in and does not replace it globally.
- Release orchestration must query a bounded, fail-closed source-gate evaluator.
- Buildx cache storage and an additional parallel runner are required when the
  source gate applies.
