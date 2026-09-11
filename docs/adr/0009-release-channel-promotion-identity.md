# Release Channel Promotion Identity

## Status

Accepted for implementation. This supplements ADR 0008 and supersedes its
channel interpretation where they differ.

## Context

ADR 0008 binds a release to a PR-local `VERSION` identity, but its initial
channel implementation recognizes only stable, beta, and dev. In particular,
the automatic stable patch path rejects a prerelease source `VERSION`. That
makes an RC indistinguishable from beta in process terms and prevents a final
stable release from using the immutable lineage already created by a
prerelease.

## Decision

The release contract has four distinct channels: stable (`X.Y.Z`), beta
(`X.Y.Z-beta.N`), RC (`X.Y.Z-rc.N`), and dev (`X.Y.Z-dev.N`). Beta, RC, and dev
are prereleases and cannot update stable `latest` or a non-prerelease GitHub
Release.

For `type:patch`, promotion is explicit and preserves the numeric base:

- a final stable source may create an automatic next stable patch, or an exact
  beta/dev version for that next patch base;
- beta may remain beta or promote only to exact RC with the same `X.Y.Z`;
- RC may remain RC or promote only to exact stable with the same `X.Y.Z`;
- dev may remain dev but cannot enter beta, RC, or stable promotion.

The beta-to-stable shortcut and all reverse/downgrade channel transitions fail
closed. Major and minor versions remain exact-version operations and continue
to use their existing monotonic version checks.

An exact version is an input to the trusted preparation controller, not an
inference. The controller reads only the PR source `VERSION`, validates the
exact value against the labels and allowed transition, reserves the matching
tag, writes it into a signed `VERSION`-only commit, and records it in frozen
provenance. Cargo manifests, tags, snapshots, queue order, and recovery state
cannot select or replace it.

Every downstream boundary validates the frozen identity pair: `Release
completion`, merged identity resolution, tag ownership, failure context and
notifier transport, and same-SHA recovery. Recovery reuses the immutable
version/channel pair and cannot promote an RC or rebuild its preparation.

## Consequences

- Maintainers receive a first-class RC label contract without treating beta as
  an RC alias.
- Final release promotion has a supported identity path and no longer depends
  on `next_patch` accepting a prerelease source.
- The external `channel:rc` GitHub label remains an owner-operated repository
  configuration item; checked-in policy support does not assert that the label
  has already been created remotely.
