# Target-only manual release from merged main

## Status

Accepted. Supersedes ADR 0015 and the reservation/completion parts of ADR 0018,
0020 and 0021.

## Context

The previous chain resolved a version from `main:VERSION`, created a signed
`VERSION`-only identity commit, reserved it with a branch ref, and required a
completion check on a pull request created by `Release Preparation` with
`GITHUB_TOKEN`. GitHub does not create workflow runs for `pull_request_target`
events caused by `GITHUB_TOKEN`, so that pull request could never satisfy its own
declared required checks. Every release since the cutover was completed through
an out-of-band owner credential, and the automated entry point never worked.

The playbook reference implementation for this Topic resolves the version from a
trusted final release baseline, uses the already-merged `main` commit as the
release source, and publishes without creating a pull request or repository-side
claim refs. The manual step is the target decision; everything else is executed
by the workflow.

## Decision

`Release` is the only entry point. It takes one `target` input and:

1. requires dispatch from `refs/heads/main`;
2. fixes the source to `github.sha` and requires a verified signature, main
   reachability, and a successful `CI (main)` run;
3. selects the highest published final GitHub Release as the baseline, reusing
   the same signature, reachability, CI and publication checks;
4. verifies a non-empty release range from the baseline source to the source;
5. resolves the canonical version from the baseline and existing prerelease
   tags, with no `VERSION` file;
6. builds one identity-bound bundle, reuses or builds images under the version
   and `sha-<source>` tags, publishes the Release and `release-images.json`, and
   fails closed on foreign or partial objects.

There is no `VERSION` file, preparation workflow, identity commit, reservation
ref, publication lock, completion check, or release abort operation.

## Consequences

- The flow needs no extra token and no owner action after the dispatch.
- Required PR checks (`Review Policy Gate`, `Quality Gates Contract`) still
  protect the product pull requests that produce the release source; they do not
  run on a synthetic release PR because none exists.
- Retrying means re-running the same dispatch or re-dispatching from the same
  `main` commit. If `main` moved, the same target fails closed on the existing
  tag instead of rewriting it.
- `VERSION` is removed; `/api/version` continues to use `APP_EFFECTIVE_VERSION`
  and `CARGO_PKG_VERSION`.
- The baseline is the published release history, not a working-tree file.
