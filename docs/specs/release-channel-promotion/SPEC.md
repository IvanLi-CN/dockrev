# Dockrev Manual Version Release Delivery

## Context and Scope

Dockrev release decisions are made through one manual `target` input, then
executed from the already-merged `main` source by the `Release` workflow. The
only allocation baseline is the published final-release history.

In scope: target input and SemVer calculation, prerelease sequencing from tags,
preflight and baseline validation, publication surfaces, artifact integrity,
failure context, and retry semantics.

Out of scope: release queues or trains, historical release identity backfill,
version files or reservation refs, synthetic release pull requests, changes to
generic Manual Version Release Delivery guidance, and release publishing as part
of a pull request.

## Terms and Interfaces

- `target`: the only required `Release` dispatch input. It is a release intent
  or one exact supported SemVer value.
- Stable: `X.Y.Z`; prerelease: `X.Y.Z-alpha.N`, `X.Y.Z-beta.N`, or
  `X.Y.Z-rc.N`.
- Trusted baseline: the highest published, non-draft, non-prerelease GitHub
  Release whose tag points at a `main`-reachable, signed source with a
  successful `CI (main)` run.
- Release source: the dispatch's `github.sha`.
- Release identity: source SHA + canonical version + release tag + bundle
  digest + published image digests.
- Policy file: `.github/manual-version-release.json`.

## Requirements

### REQ-MVRD-001: Single target decision

`Release` MUST expose exactly one required string input named `target`. It MUST
accept only `major`, `minor`, `patch`, `alpha`, `beta`, `rc`, or strict exact
SemVer in the supported stable and prerelease forms. Exact input MUST have no
`v` prefix, leading zeroes, build metadata, or unsupported prerelease channel.
Other values MUST fail closed.

### REQ-MVRD-002: Final-release allocation

Every dispatch MUST resolve the canonical version from the trusted baseline and
`target` only. For baseline `X.Y.Z`, `major` MUST calculate `(X+1).0.0`,
`minor` MUST calculate `X.(Y+1).0`, and `patch` MUST calculate `X.Y.(Z+1)`.
`VERSION` MUST NOT be a release input. There MUST be no working-tree counter,
merge order, PR count, or label input.

An exact stable value MUST equal one of the calculated numeric targets or a
same-core RC-to-stable promotion. An exact prerelease value MUST equal the next
valid channel target from the baseline and existing prerelease tags.

### REQ-MVRD-003: Prerelease sequence

From a stable baseline, `alpha`, `beta`, and `rc` MUST start on the next patch
core at sequence `.1` of the next lineage; a stable baseline MAY start any
prerelease channel directly. Within one core, alpha MAY increment or advance to
beta, beta MAY increment or advance to RC, and RC MAY increment or promote to
stable. RC promotion MUST retain the same core. Reverse transitions and skipped
transitions within a prerelease lineage MUST fail. Prereleases MUST never
advance stable `latest`.

### REQ-MVRD-004: Preflight and source binding

Release MUST fail closed unless the dispatch ref is `refs/heads/main`, the
source commit has a verified signature, the source is reachable from `main`, and
`CI (main)` has a successful completed run for the source. The baseline release
MUST be signed, `main`-reachable and final, and its published assets MUST remain
verifiable. A baseline that predates `CI (main)` MAY be accepted because it only
supplies the numeric baseline; a baseline with failing `CI (main)` runs MUST fail
closed. The range from the baseline source to the release source MUST be
non-empty and fully comparable.

`Release` MUST NOT create or merge a pull request, write to `main`, or create a
reservation, publication lock, or completion ref. Because `GITHUB_TOKEN` cannot
create a ref that introduces workflow changes, the version tag MUST exist before
dispatch and MUST point at the release source; a missing or foreign tag MUST
fail closed.

### REQ-MVRD-005: Publication, integrity, and retry

Stable releases MUST publish a GitHub Release and versioned GHCR images, then
advance `latest` only after the version tag is verified. Alpha, beta, and RC MUST
publish a GitHub prerelease and versioned GHCR images without advancing `latest`.

The workflow MUST build one identity-bound bundle containing all binary and
packaged assets, and MUST verify bundle identity, file hashes, and the GitHub
artifact SHA-256 before publication. Images MUST be built once under the version
and `sha-<source>` tags; existing tags MUST be reused only when both resolve to
the same digest, and partial or foreign tags MUST fail closed. Publication MUST
record both image digests in `release-images.json`.

Failure context MUST include the target, source SHA, and the resolved version,
channel, tag, and artifact digest when they exist, plus the retry instruction. A
retry MUST reuse the same dispatch source; a dispatch after `main` moves is a
new intent and MUST fail closed on an existing tag instead of rewriting it.

## Verification

### VER-MVRD-001: Target decision and baseline

- Method: `bash .github/scripts/release-channel-contract-check.sh` and direct
  policy fixtures.
- Covers: `REQ-MVRD-001`, `REQ-MVRD-002`.
- Pass condition: only the `target` dispatch input exists; baseline `0.81.0`
  computes major `1.0.0`, minor `0.82.0`, patch `0.81.1`, alpha
  `0.81.1-alpha.1`, beta `0.81.1-beta.1`, and rc `0.81.1-rc.1`; no `VERSION`
  file is read.

### VER-MVRD-002: Channel sequencing and publication

- Method: policy fixtures and static release workflow contract.
- Covers: `REQ-MVRD-003`, `REQ-MVRD-005`.
- Pass condition: starts, increments and allowed promotions pass; reverse,
  skipped and unsupported transitions fail; prereleases never update `latest`.

### VER-MVRD-003: Preflight, identity and retry

- Method: intent, baseline, range, bundle, digest, and failure-context fixtures.
- Covers: `REQ-MVRD-004`, `REQ-MVRD-005`.
- Pass condition: a wrong ref, unsigned source, unreachable source, missing
  `CI (main)`, missing baseline, or empty range fails closed; identity-bound
  bundle and image digests stay unchanged across a retry.

### VER-MVRD-004: Project snapshot

- Method: Style Playbook catalog sync, topic rebuild, candidate discovery, and
  audit.
- Covers: the checked-in Dockrev project snapshot for this topic.
- Pass condition: the snapshot describes the checked-in manual release policy
  and all catalog checks pass.

## Related ADRs

- [0022-target-only-manual-release](../../adr/0022-target-only-manual-release.md)

## References

- [IMPLEMENTATION.md](./IMPLEMENTATION.md)
- [HISTORY.md](./HISTORY.md)
- [Maintainer release guide](../../release.md)
