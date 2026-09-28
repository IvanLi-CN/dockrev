# Dockrev Version Release

Dockrev uses one manual version decision and immutable release identities. The
checked-in policy is `.github/manual-version-release.json`; failure routing and
required PR checks are declared in `.github/release-failure-notification.json`
and `.github/quality-gates.json`.

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
using Release Preparation. The repository currently records `0.81.0`.

For a baseline `X.Y.Z`, the numeric decisions calculate:

| Input | Target |
| --- | --- |
| `major` | `(X+1).0.0` |
| `minor` | `X.(Y+1).0` |
| `patch` | `X.Y.(Z+1)` |

From a stable baseline, `alpha` and `beta` start at the next patch core with
sequence `.1`. Alpha can increment or advance to beta. Beta can increment or
advance to RC. RC can increment or promote to stable on the same core. A stable
baseline may start beta directly; RC cannot start directly from stable. Exact
prerelease inputs must match the next valid target, and an exact stable input
must match a calculated numeric target or the same-core RC promotion.

## Identity and Publication

Preparation freezes the current main commit, baseline, input, and target. It
creates a signed, single-parent commit that changes only root `VERSION`, then
reserves that target with `release-reservation/v<VERSION>`. The
`release-preparation/v<VERSION>` branch is opened as a PR to `main`; the
existing branch protection and `Release completion` check remain the merge
boundary. GitHub places pull request workflows created with `GITHUB_TOKEN` in
an approval-required state; a user with write access must approve those runs
from the PR before the checks can complete ([GitHub Actions event behavior](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows)).

After the identity PR merges, `Release` resolves the signed provenance from the
merged commit and rechecks the direct immutable reservation. It builds one
identity-bound bundle containing all architecture binaries, release archives,
checksums, and a content manifest. The GitHub artifact SHA-256 and the bundle's
file hashes are checked before publication.

- Stable versions publish a GitHub Release, versioned GHCR images, and the
  `latest` image tags.
- Alpha, beta, and RC versions publish GitHub prereleases and versioned GHCR
  images. They never advance `latest`.

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
same immutable identity.

Release failures upload structured context with the verified identity and the
artifact digest when one exists. The selected Oidrune/OIDC notification route
includes the recovery instruction. Other expected-success workflow failures
use the generic notification route and do not claim a release identity.

There is no label-based release decision, historical identity backfill, release
queue, or release train.

## Local Contract Check

Run the deterministic release contract from the repository root:

```sh
bash .github/scripts/release-channel-contract-check.sh
```
