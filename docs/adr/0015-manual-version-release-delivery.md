# Manual Version Release Delivery

## Status

Accepted. Supersedes ADRs 0008, 0009, 0010, 0011, and 0012 for release
decision, allocation, channel progression, identity, and recovery behavior.

## Context

The prior release path selected decisions from mutable PR labels and used
published GitHub releases as its allocation baseline. Its channel, historical
backfill, and recovery rules were spread across policy files, scripts, and
workflow inputs. Dockrev needs one auditable version decision and a single
repository value that advances with each accepted release identity.

## Decision

`Release Preparation` accepts one required string input named `version`. Its
allowed values are `major`, `minor`, `patch`, `alpha`, `beta`, `rc`, or an
exact canonical SemVer value in the stable, alpha, beta, and RC forms. The
value has no `v` prefix, leading zeroes, or build metadata. The `dev` channel
is unsupported.

The allocator reads the current root `VERSION` on `main` for every new
decision. A missing, empty, or invalid file fails closed. From baseline
`X.Y.Z`, numeric intents calculate major `(X+1).0.0`, minor `X.(Y+1).0`, and
patch `X.Y.(Z+1)`. From a stable baseline, alpha and beta begin at the next
patch core with sequence `.1`; beta may start directly. Alpha advances to beta,
beta advances to RC, and RC promotes to stable, with each prerelease transition
retaining its core. Same-channel decisions increment their sequence. Stable
exact input must match a numeric calculated target or same-core RC promotion.
Exact prerelease input must be the next valid target. Direct stable-to-RC,
skipped transitions, and reverse transitions fail closed.

Preparation freezes the main SHA, baseline, decision, and target in a signed,
single-parent commit that changes only root `VERSION`. An immutable version
reservation points directly to that commit. A repeated dispatch reuses an
existing matching identity while its baseline remains current; a foreign
reservation fails closed. The identity moves through the existing protected
main PR and completion check. Once merged, Release publishes the frozen
identity, and the next decision reads the updated main `VERSION`.

One identity-bound artifact bundle supplies all architecture binaries,
archives, checksums, and container image inputs. Publication verifies the
bundle manifest, file hashes, and GitHub artifact SHA-256. Stable releases
publish GitHub Release and versioned GHCR images and may advance `latest` only
when they are the newest stable version. Alpha, beta, and RC publish
prereleases and versioned GHCR images without advancing `latest`.

Failure notification carries the resolved identity and artifact digest when
available. Same-identity recovery reuses a complete existing bundle and its
digest. If publication failed before a complete bundle existed, recovery may
build assets again for that same identity. Recovery never recalculates the
version. Historical identity backfill, release queues, and release trains are
not supported.

The sole release policy file is `.github/manual-version-release.json`. The
generic Style Playbook Manual Version Release Delivery topic remains unchanged;
the Dockrev project snapshot records the repository-specific implementation.

## Consequences

- The root `VERSION` file must exist and be valid before release preparation.
- Version intent, target, baseline, identity, and recovery all have a
  deterministic repository contract.
- Release Preparation-created PR workflows require approval by a user with
  write access because the PR is opened with `GITHUB_TOKEN`.
- Existing label-driven policy, label-gate checks, and historical backfill
  exceptions no longer define current release behavior.
