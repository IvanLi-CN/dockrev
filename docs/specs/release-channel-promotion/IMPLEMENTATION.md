# Dockrev Manual Version Release Delivery Implementation

## Current Status

- Implementation: complete in the checked-in policy, workflows, helpers,
  deterministic fixtures, and maintainer documentation.
- Lifecycle: active.
- Main allocation baseline: root `VERSION`; currently `0.81.0`.
- Generic Style Playbook topic: unchanged. Only the Dockrev project snapshot
  and its catalog indexes are in scope for Style Playbook synchronization.

## Implementation Coverage

- `REQ-MVRD-001` and `REQ-MVRD-002`: `.github/manual-version-release.json`,
  `.github/scripts/release_policy.py`, and
  `.github/scripts/release_preparation.py` define the single `version` input,
  calculate targets from `main:VERSION`, and reject missing or invalid bases.
- `REQ-MVRD-003`: policy calculation and release workflow channel selection
  enforce alpha/beta/RC progression and prevent prereleases from advancing
  stable `latest`.
- `REQ-MVRD-004`: preparation writes a signed `VERSION`-only identity,
  reserves the version directly to that identity, reuses a matching existing
  identity, and opens the protected main PR. The completion workflow validates
  the frozen baseline and identity before merge.
- `REQ-MVRD-005`: Release resolves merged identity, verifies the immutable
  reservation and artifact bundle, publishes the channel-specific GitHub and
  GHCR surfaces, records identity-aware failure context, and supports
  same-identity recovery.
- `.github/release-failure-notification.json` and the generic failure
  sidecar no longer classify the removed label gate as an expected-success
  workflow.

## Validation

- `bash .github/scripts/release-channel-contract-check.sh`
- `python3 .github/scripts/test_workflow_failure_notification_contract.py`
- `actionlint` on the changed workflow files
- `python3 bin/spec_contract_check.py --path docs/specs/release-channel-promotion/SPEC.md`
- Style Playbook project snapshot sync and catalog audit

The Release Preparation workflow creates its PR with `GITHUB_TOKEN`. GitHub
requires a repository writer to approve the resulting PR workflow runs before
they execute. No extra repository credential or external permission change is
part of this implementation.

## Remaining Gaps

- No live release, tag, GHCR publication, or recovery dispatch is run as part
  of deterministic implementation validation.
- The Dockrev snapshot update must complete its own Style Playbook repository
  validation and review flow.

## References

- [SPEC.md](./SPEC.md)
- [HISTORY.md](./HISTORY.md)
