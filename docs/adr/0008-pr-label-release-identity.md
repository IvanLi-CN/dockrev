# PR Label Release Identity

## Status

Accepted for implementation.

## Context

The previous release path selected the oldest eligible mainline snapshot from
git notes and coordinated queue/readiness/recovery state across workflows. That
made version allocation depend on mutable history and made a missing receipt a
publication concern. It also allowed release preparation to happen after the
product PR had already merged.

## Decision

Use the PR label and a root `VERSION` file as the release intent contract. A
trusted `Label Gate` requires exactly one release type and channel. After full
source PR CI succeeds, `Release Preparation` creates a `VERSION`-only,
single-parent preparation commit on the same PR branch using
`createCommitOnBranch(expectedHeadOid)`. Its signed trailers bind the source
SHA, version, and frozen labels. `Release completion` is the merge-readiness
check and rejects head drift, invalid trailers, unsigned commits, source-check
failures, and tag conflicts.

The mainline `Release` workflow consumes only the merged SHA and immutable
preparation or `version-only-release-pr` provenance. It verifies tag ownership,
builds both images and binary assets, and publishes the corresponding GitHub
Release. FIFO selection, queues/trains, snapshot backfills, mutable label
reconstruction, tag rewriting, and successor-version recovery are forbidden.
Manual dispatch is limited to same-SHA recovery for an existing identity. A
historical merge without identity is repaired by one non-empty `VERSION`-only
release PR carrying `Covered-Product-Merge-SHA`.

Failure notification keeps the selected OIDC/Oidrune transport. Release emits
a structured context artifact and the sidecar validates all identity, asset,
URL, and recovery fields before invoking the reusable notifier. The local gate
declares no required secrets; OIDC allowlists and GitHub ruleset alignment are
owner-operated external configuration.

## Consequences

- Version allocation is local to the product PR and cannot be reordered by a
  mainline queue.
- Preparation commits add a small, predictable CI exception (`VERSION` only)
  while preserving signed-commit and PR-only policy.
- A missing historical identity has an explicit, auditable PR boundary instead
  of an implicit backfill or recovery path.
- Publication recovery is narrow and repeatable because it reuses the same
  immutable merge SHA, version, tag, and provenance.
