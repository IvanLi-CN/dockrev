# Dockrev Release Contract

Dockrev releases are identity-bound to one merged product PR. The checked-in
contracts are `.github/pr-label-release.json`, `.github/release-failure-notification.json`,
and `.github/quality-gates.json`; the offline validation entrypoint is
`.github/scripts/release-channel-contract-check.sh`.

## Release identity

`VERSION` at the repository root is the only numeric version source. Cargo
manifest versions, existing tags, environment variables, and commit order do
not allocate a release version.

Every product PR targeting `main` must carry exactly one `type:*` label and one
`channel:*` label. The supported values are:

- `type:patch`, `type:minor`, `type:major`, or `type:none`;
- `channel:stable`, `channel:beta`, `channel:rc`, or `channel:dev`.

Stable versions are `X.Y.Z`; beta, RC, and dev versions are respectively
`X.Y.Z-beta.N`, `X.Y.Z-rc.N`, and `X.Y.Z-dev.N`. All non-stable channels create
GitHub prereleases and versioned GHCR images, never `latest`. `type:none` is valid policy input but does not
create a release identity and cannot change an existing `VERSION`. The one
bootstrap exception is the initial non-product PR that adds a non-empty root
`VERSION` when the base branch has no `VERSION` yet; `Release completion` verifies
that absence before accepting it.

## Normal product PR

1. The source head must pass the complete `CI (PR)` workflow and `Label Gate`.
2. `Release Preparation` reads the source `VERSION`. A stable patch release
   from a final `VERSION` uses the next patch. Major/minor, beta, RC, dev, and
   prerelease-to-stable promotion require an exact version input; the exact
   value is verified against the source `VERSION`, frozen labels, signed
   trailers, branch head, and tag reservation before it can become identity.
   For `type:patch`, the only prerelease promotion sequence is
   `X.Y.Z-beta.N -> X.Y.Z-rc.N -> X.Y.Z`; each step keeps `X.Y.Z` unchanged.
   Beta cannot promote directly to stable, dev does not promote into the
   beta/RC/stable sequence, and RC cannot return to beta or dev. Before
   writing, it atomically creates the shared
   `release-reservation/vVERSION` ref, pointing to an owner-stamped reservation
   commit whose trailers bind the PR and source SHA. Completion validates that
   immutable ref ownership before accepting the identity.
3. Preparation uses GitHub's `createCommitOnBranch(expectedHeadOid)` to add one
   signed, single-parent commit that changes only `VERSION`. Its trailers bind
   the source SHA, product version, label intent, and
   `Release-Mode: normal-preparation`.
4. The trusted `Release completion` workflow, checked out from `main`, emits
   the single required check. It revalidates the source checks, PR base, labels,
   trailers, signature, branch head, and tag reservation. The
   preparation commit changes only `VERSION`; if the PR workflow observes that commit,
   `Release Preparation` recognizes its signed trailers and skips a second
   preparation, so the source identity cannot recurse.
5. After merge, `Release` resolves the merged SHA to exactly one merged PR and
   consumes only its immutable identity. It verifies tag ownership, builds
   `dockrev` and `dockrev-supervisor` for amd64/arm64 and gnu/musl, then
   publishes the GitHub Release and GHCR images.

## Remediation boundaries

`workflow_dispatch` on `Release` requires an existing release-enabled immutable merged identity,
the same merge SHA, a non-empty recovery reason, and a prior failed automatic
`Release` run for that SHA. It retries publication only; it cannot write
`VERSION`, create a new PR, select a successor version, rewrite a tag, or
publish another PR.

If a historical product merge has no identity, create exactly one non-empty
`VERSION`-only PR with `Release-Mode: version-only-release-pr`, a
`Covered-Product-Merge-SHA` trailer, the product version, and frozen label
intent. Its signed type/channel intent is validated from the recovery PR, and
its product version is validated against the immutable `VERSION` at
`Covered-Product-Merge-SHA` by the same promotion policy. `Release completion` validates that boundary and the normal merged
identity path handles publication. This PR is not a same-SHA recovery.

The release channel is frozen in the preparation or version-only provenance.
`Release completion`, merged-identity resolution, tag ownership, failure
context, notifier transport, and same-SHA recovery all revalidate the same
version/channel pair. A recovery never recomputes an RC or converts it into a
stable version. RC is always a prerelease and never advances the stable
`latest` surface.

FIFO queues, release trains, snapshot backfills, mutable label reconstruction,
and historical tag repair are deliberately unsupported. A stale reservation
ref fails closed and requires maintainer cleanup after confirming that no
identity uses its version.

## Failure notification

The `Release` workflow uploads `release-failure-context.json` with the release
intent, source and merge SHAs, version, tag, asset names, run URL, and exact
same-SHA recovery instruction. Identity resolution failures emit a marked
`unknown/unknown` fallback context from the checked-out `VERSION` so they are not silent; if no
valid immutable version is available, the context job fails closed instead of
inventing a sentinel version. A resolver result that explicitly proves a historical product merge has no
identity must be repaired by creating the single `VERSION`-only release PR for
`Covered-Product-Merge-SHA`; it must not use the same-SHA recovery dispatch. A
resolver error, such as a transient API or checkout failure, is classified
separately and tells the maintainer to retry `Release` after verifying the
merged identity; it must not create a new release identity from an unresolved
error.
`Notify failed release` validates every field before invoking the selected OIDC/Oidrune reusable notifier. The repo-local
transport gate declares `required_secrets: []`; OIDC allowlists and ruleset
alignment remain owner actions outside this repository change.

## Owner actions

Maintainers must align the `main` ruleset with `Review Policy Gate`, `Label
Gate`, and `Release completion`, require PR-only signed commits, and allow
job-scoped `contents: write` only where the workflows declare it. They must
also confirm the OIDC subject/audience allowlist for Oidrune. No workflow in
this change performs those external configuration writes.
