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
  `.github/scripts/release_policy.py` enforce the exact promotion transition.
  `.github/scripts/release_baseline.py` selects and revalidates the highest
  qualified final release across legacy and canonical tags, rejects conflicting
  qualified tag targets, and carries that frozen allocation input across
  preparation, completion, and identity resolution.
- `REQ-RELEASE-CHANNEL-003`: `.github/scripts/release_identity.py`,
  `.github/scripts/release_completion.py`,
  `.github/scripts/release_failure_context.py`, and `.github/workflows/release.yml`
  carry the immutable channel/version pair through publication and recovery.
- `REQ-RELEASE-CHANNEL-004`: `.github/scripts/release_preparation.py` and
  `.github/workflows/release-preparation.yml` expose the explicit
  `version-only-release-pr` mode and bind its covered merge, exact version,
  baseline, intent, signed VERSION-only commit, reservation, and recovery ref.
  Recovery branches use the `recovery/` selector; automatic workflow-run
  preparation skips them and the helper rejects version-only preparation on
  other branches. `.github/workflows/release-completion-pr.yml` contains only
  the trusted `pull_request_target` path.
- Explicit version-only retries reuse an existing signed single-parent
  identity and validate its source-parent CI and trusted `pull_request_target`
  Label Gate evidence. The final identity resolver independently rejects a
  reserved version-only identity with zero or multiple parents.
- The baseline resolver and the publication workflow share a five-hop maximum
  for nested annotated tags: a commit reached on the fifth annotated hop is
  accepted, while a sixth hop fails closed.

## Coverage / rollout summary

- The checked-in policy accepts `channel:rc`; creation of the corresponding
  remote GitHub label and any ruleset/OIDC alignment remain maintainer actions.

## Remaining Gaps

- No release, tag, artifact, image, deployment, or recovery dispatch is part of
  this implementation.
- The approved historical boundary is PR #391 merge
  `978207fe9d140d81e2d4a2a7bd24fb253a04ebff` and one `0.80.2` stable identity;
  PR #390 receives no separate release identity.

## References

- `./SPEC.md`
- `./HISTORY.md`
