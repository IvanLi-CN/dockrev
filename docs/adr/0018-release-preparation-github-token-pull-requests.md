# Automated release preparation keeps opening its identity PR with `GITHUB_TOKEN`

## Status

Superseded by ADR 0022; no pull request is created by the release workflow.

## Context

`Release Preparation` opens the `VERSION`-only identity pull request with the
workflow `GITHUB_TOKEN`. GitHub refuses that with HTTP 403 unless the repository
enables "Allow GitHub Actions to create and approve pull requests". The switch
is exposed only through an endpoint that requires a classic `repo` scope, so a
workflow cannot read or verify it.

The alternative was to stop opening the PR from the workflow and print a compare
URL for the owner. It was rejected because it adds a manual step to every
release, while the version decision and the dispatch are already owner actions.

## Consequences

- The repository setting is an owner prerequisite, documented in
  `docs/release.md`, not a gate. No checked-in check can observe it.
- The workflow fails with an explicit English hint and the generic failure
  notification reports the failed run, so a flipped switch is visible within one
  dispatch instead of silently blocking releases.
- Enabling the switch also lets any workflow with `pull-requests: write` approve
  pull requests. `Review Policy Gate` remains the review contract and exempts the
  repository owner and `admin`/`maintain` authors.
- A failed preparation leaves the identity commit and reservation in place; a
  re-run with the same `version` input reuses them.
