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
- Inputs: source `VERSION`, frozen type/channel labels, and exact version
  input where required.
- Outputs: a signed `VERSION`-only identity or a fail-closed validation error.
- covers: `G1`, `G2`

### REQ-RELEASE-CHANNEL-003

- The system MUST preserve the frozen RC identity through completion, merged
  identity resolution, tag ownership, failure context/transport, and same-SHA
  recovery. RC MUST remain a prerelease and MUST NOT advance stable `latest`.
- A `VERSION`-only historical identity recovery MUST preserve its signed
  release intent in an immutable reservation identity record, validate its
  explicit version against the covered merge `VERSION`, and reject covered
  versions with an existing reservation or tag. A separate immutable index
  keyed by the covered merge MUST bind exactly one recovery identity, so
  distinct successor versions cannot recover the same product merge; it MUST
  NOT choose an arbitrary successor.
- Inputs: immutable merged provenance and its version/channel pair.
- Outputs: channel-consistent publication or recovery behavior.
- covers: `G2`, `G3`

## Verification

### VER-RELEASE-CHANNEL-001

- Method: focused policy fixtures and trusted workflow contract checks.
- covers: `REQ-RELEASE-CHANNEL-001`, `REQ-RELEASE-CHANNEL-002`
- Pass condition: beta, RC, stable, and dev labels/version forms are accepted
  only in their allowed combinations; unknown, duplicate, missing, incompatible,
  reverse, and shortcut transitions fail.

### VER-RELEASE-CHANNEL-002

- Method: release identity and failure-context fixtures plus static checks of
  `release.yml`.
- covers: `REQ-RELEASE-CHANNEL-003`
- Pass condition: an RC identity keeps `-rc.N`, is marked prerelease, omits
  latest, and its recovery/failure context retains the same identity.

## Related ADRs

- [0008-pr-label-release-identity](../../adr/0008-pr-label-release-identity.md)
- [0009-release-channel-promotion-identity](../../adr/0009-release-channel-promotion-identity.md)

## References

- `./IMPLEMENTATION.md`
- `./HISTORY.md`
