# Dockrev Version Release

Dockrev uses one manual version decision and immutable release identities. The
checked-in policy is `.github/manual-version-release.json`; failure routing and
required PR checks are declared in `.github/release-failure-notification.json`
and `.github/quality-gates.json`.

## Owner Prerequisites

`Release Preparation` opens the identity pull request with `GITHUB_TOKEN`. The
repository must allow that under Settings > Actions > General > Workflow
permissions > "Allow GitHub Actions to create and approve pull requests". GitHub
only exposes that switch through an endpoint that needs a classic `repo` scope,
so no workflow can verify it; Release Preparation fails with an explicit
English hint instead, and the declared generic failure notification reports the
failed run. Enabling the switch also lets workflows approve pull requests;
`Review Policy Gate` stays the review contract and already exempts the
repository owner and `admin`/`maintain` authors.

## Version Decision

Run `Release Preparation` from `main`. Its only required input is the string
`version`, which accepts `major`, `minor`, `patch`, `alpha`, `beta`, `rc`, or an
exact canonical SemVer value:

- Stable: `X.Y.Z`.
- Prerelease: `X.Y.Z-alpha.N`, `X.Y.Z-beta.N`, or `X.Y.Z-rc.N`.

Values have no `v` prefix or build metadata. Leading zeroes are rejected.
The `dev` channel is unsupported.

The allocator reads the current root `VERSION` from `main` for every dispatch.
It fails closed if that file is missing, empty, or invalid. Bootstrap the
repository through a protected change that adds a valid root `VERSION` before
using Release Preparation. The baseline is always that protected `main` value;
no tag, package manifest, or historical release substitutes for it.

For a baseline `X.Y.Z`, the numeric decisions calculate:

| Input | Target |
| --- | --- |
| `major` | `(X+1).0.0` |
| `minor` | `X.(Y+1).0` |
| `patch` | `X.Y.(Z+1)` |

Numeric decisions always calculate from the numeric core of the latest main
`VERSION`, including when that baseline is a prerelease. An exact stable input
must match one of those calculated targets or promote the current RC on its
same core. The alpha/beta/RC progression applies to prerelease channel inputs.

From a stable baseline, `alpha`, `beta`, and `rc` start at the next patch core
with sequence `.1`. Alpha can increment or advance to beta. Beta can increment
or advance to RC. RC can increment or promote to stable on the same core. A
stable baseline may start alpha, beta, or rc directly. Exact prerelease inputs
must match the next valid target, and an exact stable input must match a
calculated numeric target or the same-core RC promotion.

## Identity and Publication

Preparation freezes the current main commit, baseline, input, and target. It
creates a signed, single-parent commit that changes only root `VERSION`, then
reserves that target with `release-reservation/v<VERSION>`. The
`release-preparation/v<VERSION>` branch is opened as a PR to `main`. The
target merge contract in `.github/quality-gates.json` requires
`Review Policy Gate`, `Manual Version Release Completion`, and
`Quality Gates Contract`, and the live ruleset requires all three. A declared
check that the ruleset does not require yet must be listed in
`pending_required_checks`; the live checker reports that state as `pending`, not
aligned, and rejects any other mismatch. It also reads `.github/workflows/release-completion-pr.yml`
from `main` and verifies the `pull_request_target` trigger, the
`github.workflow_sha` checkout, and the target check name.
`Quality Gates Contract` runs from its trusted workflow source as well, so a
pull request cannot rewrite the checker that validates the repository rules. GitHub then
requires each declared check to succeed on the PR before merge. A `GITHUB_TOKEN`
pull request still needs a write-access user to approve its workflow runs
([GitHub Actions event behavior](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows)).
Release version decisions remain in the `version` dispatch input and do not
depend on PR labels. Do not mark `pending_required_checks` as enforced until the
ruleset requires them.

The completion check reports `not-applicable` for ordinary PRs with no
`VERSION` change or release-identity trailers. `VERSION` changes without a
valid release identity, and partial identity trailers, fail closed.

After the identity PR merges, `Release` resolves the signed provenance from the
merged commit and rechecks the direct immutable reservation. Before it reserves
a tag, it verifies that every enforced check of the publication gate reported
`success` on the release identity commit and that this commit is a parent of the
merged commit or the merged commit's pull request head, so a merge commit,
squash, or rebase merge of the identity PR all bind the same evidence. It then
builds one identity-bound bundle
containing all architecture binaries, release archives, checksums, and a content
manifest. The GitHub artifact SHA-256 and the bundle's file hashes are checked
before publication.

- Stable versions publish a GitHub Release, versioned GHCR images, and the
  `latest` image tags.
- Alpha, beta, and RC versions publish GitHub prereleases and versioned GHCR
  images. They never advance `latest`.

Publication records both GHCR image digests in `release-images.json`, binds it
to the release identity, attaches it to the GitHub Release, and uploads it as
the `release-images-<merge sha>` run artifact. The version tag and `latest` are
the only moving pointers to an already recorded digest.

The root `VERSION` advances when the identity PR merges, so the next decision
uses the new main baseline. Missing or invalid `VERSION` is not replaced by a
release tag or package manifest.

## Recovery and Failure

`Release` manual dispatch accepts only an existing merged `merge_sha` and a
required `recovery_reason`. It verifies that the same identity previously
failed. When that run produced the complete release bundle, recovery reuses
that bundle and verifies the same artifact digest before publishing; it never
recalculates the version or rebuilds a completed bundle. If the failure
happened before a complete bundle existed, the workflow can rebuild for that
same immutable identity. Recovery reads the original failure context to tell
those cases apart. If a digest was recorded but the original bundle is missing
or expired, it stops and asks maintainers to restore that exact bundle before
retrying.

Recovery also reuses the recorded image digests: it verifies that the live
registry tags still resolve to them and skips the image build, so one immutable
identity cannot produce two container image digests.

Release failures upload structured context with the verified identity and the
artifact digest when one exists. The selected Oidrune/OIDC notification route
includes the recovery instruction. Other expected-success workflow failures
use the generic notification route and do not claim a release identity.

There is no label-based release decision, historical identity backfill, release
queue, or release train.

## Abandoning an Identity

`Release abort` accepts a `version` and a `reason`. It refuses to run when the
identity is merged into `main`, when `v<VERSION>` or `<VERSION>` already exists
as a tag, when a GitHub Release exists, when a publication lock exists, or when
the reservation points at another identity. Otherwise it closes the identity PR
and deletes `release-preparation/v<VERSION>` and
`release-reservation/v<VERSION>`, which frees the version for a new preparation
run. It never deletes tags, GitHub Releases, or published images, so a failed
abort leaves the published identity untouched and an already published identity
cannot be aborted at all.

## Local Contract Check

Run the deterministic release contract from the repository root:

```sh
bash .github/scripts/release-channel-contract-check.sh
```
