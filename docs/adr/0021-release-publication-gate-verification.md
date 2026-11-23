# Publication re-verifies the declared gate on the merged identity

## Status

Accepted. Extends ADR 0015.

## Context

`Release` trusted the merge: it resolved the identity and published. The
declared `required_checks` were enforced by the live ruleset, but the release
workflow itself never proved that those checks had succeeded on the commit it
was about to publish. `CONTEXT.md` also called the Dockerfile/Compose source
build a "release-blocking" gate even though `Deploy Smoke` is informational and
not re-checked at publication.

## Considered options

Promoting `Deploy Smoke` to a required check was rejected: it adds a long smoke
run to every Docker-affecting pull request for a signal the source commit's CI
already produced.

## Consequences

- `Release` reads `.github/quality-gates.json`, excludes `pending_required_checks`,
  and requires `success` for every remaining check on the release identity commit,
  which must be a parent of the merged commit or that commit's pull request head,
  before it reserves a tag. GitHub reports required checks on the pull request
  head, not on the merge commit, so the identity commit is the only place that
  evidence exists; accepting the pull request head keeps squash and rebase
  merges of the identity PR valid.
- A missing or failing check fails closed and produces the usual identity-aware
  failure context.
- `CONTEXT.md` now defines the source-build smoke as an informational PR-time
  signal, and publication eligibility as the declared publication gate.
- The gate stays declared in one place: adding `Quality Gates Contract` to the
  live ruleset needed no release change.
