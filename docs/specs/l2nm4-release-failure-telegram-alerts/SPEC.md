# Dockrev release failure Telegram alerts

## Context and Scope

### Context

Dockrev has a repository-local wrapper for failed `Release` notifications. The
shared Telegram reusable workflow is being replaced by the Oidrune reusable
workflow, whose caller contract accepts an `outcome` and a complete `summary`.
The wrapper must continue to identify the real release target SHA when release
queue or manual backfill runs use a different target than the triggering run
SHA.

### In scope

- `.github/workflows/notify-release-failure.yml`
- `.github/scripts/release-channel-contract-check.sh`
- `docs/specs/l2nm4-release-failure-telegram-alerts/`
- The Oidrune reusable workflow reference and caller-side notification summary

### Out of scope

- Oidrune gateway control-plane configuration, allowlists, deployment, or
  other repository-external configuration
- Dockrev release queue behavior, skip logic, publication, or application code
- A real Telegram smoke notification

## Requirements

- **REQ-TRIGGER:** The wrapper MUST listen for `Release` `workflow_run`
  `completed` events on `main` and MUST notify only when the run conclusion is
  `failure`.
- **REQ-SMOKE:** The wrapper MUST retain a no-input `workflow_dispatch` smoke
  path that invokes notification only and does not start a release.
- **REQ-TARGET-SHA:** Failure notification MUST prefer a valid 40-character
  release target SHA parsed from failed release job logs and MUST fall back to
  `workflow_run.head_sha` when parsing or API access fails.
- **REQ-OIDRUNE-PIN:** Both notification jobs MUST call
  `IvanLi-CN/oidrune/.github/workflows/notify.yml` at the complete trusted
  release SHA `e48822f99c6402a753ed86557ea029754cbab20b`; callers MUST omit
  `gateway_url` and `oidc_audience` so Oidrune defaults apply.
- **REQ-OIDC:** Each Oidrune caller job MUST have `id-token: write` and MUST
  omit the legacy `SHOUTRRR_URL` secret forwarding.
- **REQ-SUMMARY:** The caller MUST provide a complete summary containing the
  repository, status, resolved target SHA, workflow run URL, workflow/event/ref,
  attempt, actor, and applicable note semantics; it MUST NOT rely on Oidrune to
  infer Dockrev metadata.
- **REQ-SIDE-EFFECTS:** The migration MUST NOT change Dockrev release queue,
  skip, publication, or other existing side effects.

## Acceptance Criteria

- A failed `Release` run on `main` reaches Oidrune with `outcome: failure` and a
  caller-generated summary containing the failed run context.
- A manual dispatch exposes the same Oidrune notification path with a smoke
  summary containing repository, smoke status, SHA, and run URL context.
- A queue or manual backfill failure displays the parsed release target SHA;
  an unavailable jobs/logs API displays the workflow run head SHA instead.
- Contract checks reject an unpinned Oidrune reference, missing OIDC
  permission, legacy secret forwarding, gateway override, or missing summary
  fields.

## Verification

- **VER-WORKFLOW-CONTRACT:** `bash ./.github/scripts/release-channel-contract-check.sh`
  parses the workflow and checks trigger, fixed reference, permissions, summary,
  secret, and gateway invariants; covers: REQ-TRIGGER, REQ-SMOKE, REQ-OIDRUNE-PIN,
  REQ-OIDC, REQ-SUMMARY, REQ-SIDE-EFFECTS.
- **VER-RESOLVER-SUMMARY:** A local mock GitHub API test covers log-derived SHA
  resolution and API-failure fallback while checking the emitted summary fields;
  covers: REQ-TARGET-SHA, REQ-SUMMARY.
- **VER-SPEC-CONTRACT:** `ADR_REFS=none bash
  /Users/ivan/.codex/skills/spec-sync/scripts/spec_drift_check.sh --base-ref
  origin/main --spec-path
  docs/specs/l2nm4-release-failure-telegram-alerts/SPEC.md` validates the
  canonical Spec stream, required companion files, and implementation-to-Spec
  traceability; covers: REQ-TRIGGER, REQ-SMOKE, REQ-TARGET-SHA,
  REQ-OIDRUNE-PIN, REQ-OIDC, REQ-SUMMARY, REQ-SIDE-EFFECTS.
- **VER-LIVE-RELEASE:** The Oidrune `v0.1.14` release and tag ref are checked
  to resolve to `e48822f99c6402a753ed86557ea029754cbab20b`; real Telegram smoke
  execution remains explicitly excluded; covers: REQ-OIDRUNE-PIN.
- **VER-OIDC-SIDE-EFFECTS:** Static permission and scope checks cover: REQ-OIDC, REQ-SIDE-EFFECTS.

## Related ADRs

- None
