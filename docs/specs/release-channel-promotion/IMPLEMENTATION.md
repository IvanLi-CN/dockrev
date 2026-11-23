# Dockrev Manual Version Release Delivery Implementation

## Current Status

- Implementation: complete in the checked-in policy, workflows, helpers,
  deterministic fixtures, and maintainer documentation.
- Lifecycle: active.
- Main allocation baseline: highest verified final GitHub Release.
- Generic Style Playbook topic: unchanged. Only the Dockrev project snapshot
  and its catalog indexes are in scope for Style Playbook synchronization.

## Implementation Coverage

- `REQ-MVRD-001` and `REQ-MVRD-002`: `.github/manual-version-release.json`,
  `.github/scripts/release_policy.py`, and `.github/scripts/release_intent.py`
  define the single `target` input and calculate the version from the trusted
  final Release and existing prerelease tags.
- `REQ-MVRD-003`: policy calculation enforces prerelease lineage progression
  and prevents prereleases from advancing stable `latest`.
- `REQ-MVRD-004`: `Release` preflight checks ref, signature, main
  reachability, `CI (main)` and the baseline; no pull request or claim ref is
  created.
- `REQ-MVRD-005`: `Release` builds one identity-bound bundle, reuses or builds
  images under the version and source tags, publishes the channel-specific
  GitHub and GHCR surfaces, records `release-images.json`, and writes
  identity-aware failure context.
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
