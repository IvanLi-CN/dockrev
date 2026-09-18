# Dockrev Release Channel Promotion

## Context and Scope

- Context: the PR label release contract needs a distinct RC channel and a
  bounded prerelease-to-stable promotion path while retaining immutable release
  identity.
- In scope: label parsing, `VERSION` channel validation, preparation,
  completion, merged identity, publication, failure context, recovery, and
  current maintainer documentation.
- Out of scope: GitHub label creation, ruleset/OIDC changes, publication,
  dispatching recovery, tag mutation, release queues, and release trains.

## Terms and Interfaces

- `beta`: `X.Y.Z-beta.N` prerelease channel.
- `rc`: `X.Y.Z-rc.N` prerelease channel; it is not an alias for beta.
- `stable`: final `X.Y.Z` channel and the only channel eligible to advance
  `latest`.
- `dev`: `X.Y.Z-dev.N` prerelease channel outside the beta-to-RC-to-stable
  promotion sequence.
- Interface: `.github/pr-label-release.json`, trusted release workflows, root
  `VERSION`, and release provenance trailers.

## Requirements

### REQ-RELEASE-CHANNEL-001

- The system MUST require exactly one recognized `channel:*` label and accept
  `stable`, `beta`, `rc`, and `dev` as distinct values.
- Inputs: product PR labels and root `VERSION`.
- Outputs: a fail-closed parsed release intent with a channel-compatible
  version.
- covers: `G1`

### REQ-RELEASE-CHANNEL-002

- The system MUST permit only patch-level `beta -> rc -> stable` promotion at
  one unchanged `X.Y.Z` base, and every beta, RC, dev, or
  prerelease-to-stable preparation MUST use an exact version input.
- Inputs: a qualified final-release baseline, source `VERSION`, frozen
  type/channel labels, and exact version input where required. A qualified
  baseline is non-draft, non-prerelease, created by release automation, and
  tagged at a `main`-reachable commit. Historical `X.Y.Z` tags and canonical
  `vX.Y.Z` tags qualify for lookup; future publication uses canonical tags, and
  conflicting qualified tags for one version MUST fail closed. Annotated tags
  MUST resolve to a commit within five tag-object hops; deeper nesting MUST
  fail closed.
- Outputs: a signed `VERSION`-only identity carrying
  `Release-Baseline-Version`, or a fail-closed validation error.
- covers: `G1`, `G2`

### REQ-RELEASE-CHANNEL-003

- The system MUST preserve the frozen RC identity through completion, merged
  identity resolution, tag ownership, failure context/transport, and same-SHA
  recovery. RC MUST remain a prerelease and MUST NOT advance stable `latest`.
- A `VERSION`-only historical identity recovery MUST preserve its signed
  release intent in an immutable reservation identity record. The canonical
  `release-reservation/v<VERSION>` ref MUST point directly to that verified
  signed identity SHA; legacy reservation commits remain read-only compatibility
  evidence. The record MUST validate its explicit version against the covered
  merge `VERSION`, and reject covered versions with an existing reservation or
  tag. A separate immutable index
  keyed by the covered merge MUST bind exactly one recovery identity, so
  distinct successor versions cannot recover the same product merge; it MUST
  NOT choose an arbitrary successor. The index MUST be allocated before the
  target-version reservation, and an orphaned index MUST fail closed.
- Normal and `VERSION`-only provenance MUST freeze a qualified final baseline
  in `Release-Baseline-Version`; existing preparation, completion, and merged
  identity resolution MUST revalidate that exact baseline instead of using the
  source or covered `VERSION` as a fallback allocator.
- Inputs: immutable merged provenance and its version/channel pair.
- Outputs: channel-consistent publication or recovery behavior.

### REQ-RELEASE-CHANNEL-004

- A historical identity repair MUST use one explicit
  `version-only-release-pr` preparation mode. It MUST bind an exact covered
  product merge SHA, exact product version, frozen baseline, and label intent
  to a signed, single-parent, `VERSION`-only commit created with
  `createCommitOnBranch(expectedHeadOid)`. Reservation, recovery-ref, and
  publication-lock ownership MUST bind that same identity SHA, and the
  canonical version reservation ref MUST point directly to the identity commit.
  A publication lock for the new product version MUST resolve to that exact
  identity SHA. A lock for the covered product's older version MUST be
  resolved against its own owning merge SHA and MUST NOT be treated as the
  owner of the new version or block an unrelated historical boundary.
  For a normal-preparation identity, its owning association MUST be exactly
  one merged PR into `main` whose head SHA equals the signed release identity
  SHA and whose base repository equals the target repository. Any other
  association MUST fail closed.
  Malformed, ambiguous, duplicate, or unverified lock ownership evidence MUST
  fail closed.
  The current
  approved repair boundaries are explicitly enumerated. They include PR #391
  merge `978207fe9d140d81e2d4a2a7bd24fb253a04ebff` -> `0.80.2` with baseline
  `0.80.1` and intent `type:patch channel:stable`, with no separate PR #390
  identity, and PR #395 merge
  `ff1b57b6835616cd3b7a95a479c0106426eb5d40` -> `0.80.3` with baseline
  `0.80.2` and the same intent. No tuple outside this allowlist is valid.
  Recovery PRs MUST use a `recovery/` head branch;
  automatic `workflow_run` preparation skips that prefix, and only the
  explicit manual version-only dispatch may prepare the identity. Retries MUST
  reuse a signed single-parent identity and its source-parent CI evidence.
  Label Gate evidence MUST come from `pull_request_target`; skipped or ordinary
  `pull_request` runs MUST NOT satisfy the contract.
- covers: `G2`, `G3`

## Verification

### VER-RELEASE-CHANNEL-001

- Method: focused policy fixtures and trusted workflow contract checks.
- covers: `REQ-RELEASE-CHANNEL-001`, `REQ-RELEASE-CHANNEL-002`
- Pass condition: beta, RC, stable, and dev labels/version forms are accepted
  only in their allowed combinations; unknown, duplicate, missing, incompatible,
  reverse, and shortcut transitions fail. A stale source is accepted only when
  its signed version is the successor of the qualified final baseline; manual,
  draft, prerelease, and off-main release candidates do not qualify.

### VER-RELEASE-CHANNEL-002

- Method: release identity and failure-context fixtures plus static checks of
  `release.yml`.
- covers: `REQ-RELEASE-CHANNEL-003`
- Pass condition: an RC identity keeps `-rc.N`, is marked prerelease, omits
  latest, and its recovery/failure context retains the same identity.

### VER-RELEASE-CHANNEL-003

- Method: baseline, identity-ownership, version-only preparation, and trusted
  workflow contract fixtures.
- covers: `REQ-RELEASE-CHANNEL-004`
- Pass condition: legacy and canonical baseline tags are accepted, conflicting
  qualified targets fail closed, unrelated tag ownership does not block a
  covered merge, and the sole version-only identity binds the covered merge,
  exact version, baseline, intent, reservation, and recovery ref.

## Related ADRs

- [0008-pr-label-release-identity](../../adr/0008-pr-label-release-identity.md)
- [0009-release-channel-promotion-identity](../../adr/0009-release-channel-promotion-identity.md)
- [0010-release-version-baseline](../../adr/0010-release-version-baseline.md)
- [0011-legacy-release-tag-baseline](../../adr/0011-legacy-release-tag-baseline.md)
- [0012-auto-update-release-identity-backfill](../../adr/0012-auto-update-release-identity-backfill.md)

## References

- `./IMPLEMENTATION.md`
- `./HISTORY.md`
