# Release evidence records container image digests

## Status

Accepted. Extends ADR 0015.

## Context

The release identity already bound the artifact bundle to a source and merge
SHA, verified the GitHub artifact SHA-256, and reused that bundle on recovery.
Container images were published by tag only: `0.81.10` and `latest` were the
sole pointers, and a same-identity recovery re-ran the image build, which could
yield a different digest for the same version.

## Consequences

- Publication writes `release-images.json` with the identity projection and both
  GHCR digests, attaches it to the GitHub Release, and uploads it as a run
  artifact so a failure that happens before the Release exists can still carry
  the digests.
- Recovery reuses the recorded digests, verifies the live registry tags still
  resolve to them, and skips the image build. One version cannot silently point
  at two images.
- A failure between the image push and the digest record leaves no digest to
  reuse; recovery then rebuilds for the same identity. That narrow window is the
  only case where one version can produce a second image digest, and it stays
  visible because no `release-images.json` exists for that run.
- The bundle manifest stays an input artifact; image digests are recorded after
  the push, so they cannot live inside the bundle.
- Deployment still resolves tags through Dockrev's managed override, which pins
  the resolved digest. The record is evidence for the identity, not a new
  deployment mechanism.
