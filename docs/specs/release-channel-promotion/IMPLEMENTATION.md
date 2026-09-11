# Dockrev Release Channel Promotion 实现状态

## Current Status

- Implementation: complete in the checked-in release policy, workflows, fixtures, and documentation.
- Lifecycle: active.
- Catalog note: RC is a first-class prerelease channel with exact beta-to-RC-to-stable promotion.

## Implementation Coverage

- `REQ-RELEASE-CHANNEL-001`: `.github/pr-label-release.json`,
  `.github/scripts/release_policy.py`, `.github/workflows/label-gate.yml`, and
  `.github/workflows/release-completion-pr.yml` define and validate the four
  channels.
- `REQ-RELEASE-CHANNEL-002`: `.github/scripts/release_preparation.py` and
  `.github/scripts/release_policy.py` enforce the exact promotion transition
  and preserve signed `VERSION` provenance.
- `REQ-RELEASE-CHANNEL-003`: `.github/scripts/release_identity.py`,
  `.github/scripts/release_completion.py`,
  `.github/scripts/release_failure_context.py`, and `.github/workflows/release.yml`
  carry the immutable channel/version pair through publication and recovery.

## Coverage / rollout summary

- The checked-in policy accepts `channel:rc`; creation of the corresponding
  remote GitHub label and any ruleset/OIDC alignment remain maintainer actions.

## Remaining Gaps

- No release, tag, artifact, image, deployment, or recovery dispatch is part of
  this implementation.

## References

- `./SPEC.md`
- `./HISTORY.md`
