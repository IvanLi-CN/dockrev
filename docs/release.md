# Dockrev Version Release

Dockrev uses one manual release decision and a trusted final-release baseline.
The checked-in policy is `.github/manual-version-release.json`; failure routing
and required PR checks are declared in `.github/release-failure-notification.json`
and `.github/quality-gates.json`.

## Version Decision

Run `Release` from `main`. Its only input is `target`, which accepts `patch`,
`minor`, `major`, `alpha`, `beta`, `rc`, or an exact canonical SemVer value:

- Stable: `X.Y.Z`.
- Prerelease: `X.Y.Z-alpha.N`, `X.Y.Z-beta.N`, or `X.Y.Z-rc.N`.

Values have no `v` prefix or build metadata. Leading zeroes are rejected. The
`dev` channel is unsupported.

The only allocation baseline is the highest published, non-draft,
non-prerelease GitHub Release whose tag points at a `main`-reachable commit and
whose source has a successful `CI (main)` run. `VERSION` is not a release input and there is no
second baseline source.

For a baseline `X.Y.Z`, numeric decisions calculate:

| Input | Target |
| --- | --- |
| `major` | `(X+1).0.0` |
| `minor` | `X.(Y+1).0` |
| `patch` | `X.Y.(Z+1)` |

Prerelease decisions read the existing `vX.Y.Z-<channel>.N` tags to find the
next lineage. A new prerelease core starts at sequence `.1`; within one core,
alpha can increment or advance to beta, beta can increment or advance to RC,
and RC can increment or promote to stable. A stable baseline may start any
prerelease channel directly on the next patch core. Reverse transitions and
skipped transitions inside a lineage fail closed. Exact prerelease input must
equal the next valid target, and exact stable input must equal a calculated
numeric target or promote an existing RC on the same core.

## Source and Preflight

`Release` is dispatch-only. The release source is the dispatch's `github.sha`.
Preflight fails closed unless:

- the dispatch ref is `refs/heads/main`;
- the source commit has a verified signature and is reachable from `main`;
- `CI (main)` has a successful completed run for the source commit;
- the baseline release satisfies the same conditions;
- the range from the baseline source to the dispatch source is non-empty and
  fully comparable through the compare API.

Required checks, review policy and signed commits protect the product pull
requests that produce that source. The release workflow neither creates nor
merges a pull request, and it never writes to `main`.

## Publication

The workflow builds one identity-bound bundle containing all architecture
binaries, release archives, checksums, and a content manifest. The GitHub
artifact SHA-256 and the bundle file hashes are verified before publication.

Images are built once with the immutable version tag and a `sha-<source>` tag.
If both tags already exist and resolve to the same digest, the workflow reuses
them; a partial or foreign tag pair fails closed. `latest` moves only for a
stable release and only after the version tag is verified.

Publication records both GHCR image digests in `release-images.json`, binds it
to the source/version identity, and attaches it to the GitHub Release together
with the archives and checksums. An existing release or tag that points at a
different source fails closed; an existing object for the same identity is
reused.

- Stable versions publish a GitHub Release, versioned GHCR images, and the
  `latest` image tags.
- Alpha, beta, and RC versions publish GitHub prereleases and versioned GHCR
  images. They never advance `latest`.

## Recovery and Failure

Retry the same identity with **Re-run jobs** on the failed run, so the dispatch
source SHA is unchanged, or re-dispatch from the same `main` commit with the
same `target`. Existing tags, Release and image digests are reused; a tag that
belongs to another source, a partial image tag pair, or a digest mismatch fails
closed.

If `main` has moved, a new dispatch is a new release intent. It resolves the
same version from the unchanged baseline and then fails closed on the existing
tag instead of rewriting it.

Release failures upload structured context with the target, source SHA, and the
resolved version, channel, tag and artifact digest when they exist. The
selected Oidrune/OIDC notification route includes the recovery instruction.
Other expected-success workflow failures use the generic notification route.

There is no label-based release decision, version file, reservation ref,
completion check, historical identity backfill, release queue, or release
train.

## Local Contract Check

Run the deterministic release contract from the repository root:

```sh
bash .github/scripts/release-channel-contract-check.sh
```
