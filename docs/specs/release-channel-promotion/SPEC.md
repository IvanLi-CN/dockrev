# Dockrev Manual Version Release Delivery

## Context and Scope

Dockrev release decisions are made through one manual version input, then
carried by an immutable signed identity through main protection, publication,
failure notification, and same-identity recovery. The only allocation baseline
is the current root `VERSION` on `main`.

In scope: version input and SemVer calculation, prerelease sequencing,
`VERSION` bootstrap and validation, identity preparation and reservation,
completion checks, publication surfaces, artifact integrity, failure context,
recovery, and maintainer documentation.

Out of scope: release queues or trains, historical release identity backfill,
changes to generic Manual Version Release Delivery guidance, and release
publishing as part of preparation.

## Terms and Interfaces

- `version`: the only required `Release Preparation` dispatch input. It is a
  release intent or one exact supported SemVer value.
- Stable: `X.Y.Z`; prerelease: `X.Y.Z-alpha.N`, `X.Y.Z-beta.N`, or
  `X.Y.Z-rc.N`.
- Main baseline: the valid contents of root `VERSION` at the latest `main`
  commit observed by preparation.
- Release identity: a signed, single-parent commit with only root `VERSION`
  changed and provenance for the source SHA, baseline, input, and target.
- Policy file: `.github/manual-version-release.json`.

## Requirements

### REQ-MVRD-001: Single version decision

`Release Preparation` MUST expose exactly one required string input named
`version`. It MUST accept only `major`, `minor`, `patch`, `alpha`, `beta`,
`rc`, or strict exact SemVer in the supported stable and prerelease forms.
Exact input MUST have no `v` prefix, leading zeroes, build metadata, or
unsupported prerelease channel. Other values MUST fail closed.

### REQ-MVRD-002: Main VERSION allocation

Every new preparation MUST read the latest `main:VERSION` and use it as the
only baseline. For baseline `X.Y.Z`, `major` MUST calculate `(X+1).0.0`,
`minor` MUST calculate `X.(Y+1).0`, and `patch` MUST calculate `X.Y.(Z+1)`.
These numeric calculations MUST use the baseline's numeric core whether the
baseline is stable or prerelease; prerelease channel progression applies to
prerelease inputs.
Missing, empty, or invalid `VERSION` MUST fail closed. A valid root `VERSION`
MUST be added through the protected mainline path before release preparation
can run when the file does not exist.

An exact stable value MUST equal one of the calculated numeric targets or a
same-core RC-to-stable promotion. An exact prerelease value MUST equal the
next valid channel target from the main baseline.

### REQ-MVRD-003: Prerelease sequence

From a stable baseline, `alpha` and `beta` MUST start on the next patch core at
sequence `.1`; a stable baseline MAY start beta directly and MUST NOT start RC
directly. Within one core, alpha MAY increment or advance to beta, beta MAY
increment or advance to RC, and RC MAY increment or promote to stable. RC
promotion MUST retain the same core. Reverse transitions and skipped
transitions MUST fail. Prereleases MUST never advance stable `latest`.

### REQ-MVRD-004: Immutable identity and mainline delivery

Preparation MUST freeze the observed main SHA, baseline, input, and target. It
MUST create a signed, single-parent identity commit changing only root
`VERSION`, and reserve the target version with one immutable ref directly to
that commit. Before reserving, preparation MUST confirm that `main:VERSION`
still matches the frozen baseline and that no different release identity PR is
open. A repeated dispatch for the same target and unchanged baseline MUST
reuse that identity; a reservation owned by another SHA MUST fail closed.
The identity MUST pass the existing main branch protection and `Release
completion` gate. `Release` MUST publish only after that identity reaches
`main`, and the next allocation MUST read the resulting main `VERSION`.
An ordinary PR with no release provenance and no `VERSION` change MUST pass
the completion check without enabling publication. Any `VERSION` change
without complete signed release provenance MUST fail closed.

### REQ-MVRD-005: Publication, integrity, and recovery

Stable identity MUST publish a GitHub Release and versioned GHCR images, then
advance stable `latest` only when that version is still the newest stable
release. Alpha, beta, and RC identity MUST publish a GitHub prerelease and
versioned GHCR images without advancing `latest`.

The release workflow MUST build one identity-bound bundle containing all
binary and packaged assets. It MUST verify bundle identity, file hashes, and
the GitHub artifact SHA-256 before publication. Failure context MUST include
the verified identity, failure run, recovery instruction, and artifact digest
when available. Same-identity recovery MUST reuse an existing completed bundle
and its digest; recovery before a complete bundle exists MAY rebuild assets
for the same immutable identity. Recovery MUST NOT recalculate or change the
version.

## Verification

### VER-MVRD-001: Version decision and baseline

- Method: `bash .github/scripts/release-channel-contract-check.sh` and direct
  policy fixtures.
- Covers: `REQ-MVRD-001`, `REQ-MVRD-002`.
- Pass condition: only the `version` dispatch input exists; baseline `0.81.0`
  computes major `1.0.0`, minor `0.82.0`, patch `0.81.1`, alpha
  `0.81.1-alpha.1`, and beta `0.81.1-beta.1`; missing or invalid main
  `VERSION` fails closed.

### VER-MVRD-002: Channel sequencing and publication

- Method: policy fixtures and static release workflow contract.
- Covers: `REQ-MVRD-003`, `REQ-MVRD-005`.
- Pass condition: alpha, beta, and RC starts, increments, and allowed
  promotions pass; invalid, skipped, reverse, and unsupported transitions
  fail; prereleases never update `latest`.

### VER-MVRD-003: Identity and recovery

- Method: identity, reservation, bundle, digest, and failure-context fixtures.
- Covers: `REQ-MVRD-004`, `REQ-MVRD-005`.
- Pass condition: repeated same-target preparation reuses the signed identity
  while its baseline remains current; a foreign reservation fails; identity
  and artifact digest stay unchanged during recovery.

### VER-MVRD-004: Project snapshot

- Method: Style Playbook catalog sync, topic rebuild, candidate discovery, and
  audit.
- Covers: the checked-in Dockrev project snapshot for this topic.
- Pass condition: the snapshot describes the checked-in manual release policy
  and all catalog checks pass.

## Related ADRs

- [0015-manual-version-release-delivery](../../adr/0015-manual-version-release-delivery.md)

## References

- [IMPLEMENTATION.md](./IMPLEMENTATION.md)
- [HISTORY.md](./HISTORY.md)
- [Maintainer release guide](../../release.md)
