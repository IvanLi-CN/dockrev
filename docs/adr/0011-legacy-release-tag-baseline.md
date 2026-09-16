# Legacy Release Tag Baseline

## Status

Accepted for implementation.

## Context

The release contract historically used both unprefixed `X.Y.Z` tags and the
canonical `vX.Y.Z` form. The baseline resolver only recognized the canonical
form, while identity ownership treated any reservation or tag with the same
version as proof that a covered merge already had an identity. Those two gaps
made an old tag appear missing in one path and made an unrelated tag or
reservation block a historical repair in another path.

The current product history has one bounded publication gap at PR #391's merge
SHA `978207fe9d140d81e2d4a2a7bd24fb253a04ebff`. PR #390 is an ancestor fix and
must not receive a separate release identity. The repair target is `0.80.2`,
with frozen baseline `0.80.1` and intent `type:patch channel:stable`.

## Decision

Baseline lookup accepts final semver tags in both `X.Y.Z` and `vX.Y.Z` form.
Historical unprefixed tags are immutable evidence only; all future publication
continues to use canonical `vX.Y.Z` tags. A qualified pair of same-version tags
that resolves to different commit SHAs fails closed.
Annotated tag traversal is bounded at five tag-object hops, matching the
publication workflow; a sixth hop fails closed.

An existing release identity is recognized only when its ownership evidence is
bound to the covered merge or identity SHA. Reservations validate their source
trailer and single parent, tags resolve to an owned SHA, publication locks
match their identity owner, and recovery refs match the expected identity. A
reservation, lock, or recovery ref with a different owner never becomes valid
through a version-only retry.

Historical repair uses an explicit `version-only-release-pr` preparation mode.
The caller supplies the covered product merge, exact version, frozen baseline,
and labels. Trusted source checks run before GitHub's
`createCommitOnBranch(expectedHeadOid)` creates the signed, single-parent,
`VERSION`-only identity. The repair reserves one recovery ref and one version
reservation for that same identity. The completion check has one trusted
`pull_request_target` execution path; `type:none` produces one non-product pass
without provenance or release identity.

This decision prepares identity only. It does not create the `0.80.2` tag,
GitHub Release, GHCR images, recovery dispatch, or external ruleset changes.

## Consequences

- `0.80.1` remains a valid frozen baseline whether its historical release uses
  an unprefixed or canonical tag.
- An unrelated old tag cannot block the #391 repair merely because it shares a
  version string; exact SHA ownership is required.
- The single `0.80.2` backfill is auditable and cannot silently become a
  successor-version allocator for another merged product PR.
- A completion workflow source change cannot execute untrusted PR code because
  only the base-checked-out `pull_request_target` path remains.
