# Implementation

## Scope

- Backend snapshot workers for cleanup inventory and deploy-check report refresh.
- Cleanup API contract migration from synchronous scan to snapshot-backed `ready/pending` reads.
- Deploy-check cached read + async refresh contract.
- Startup reconciliation for missing discovery projects and their linked Stack records.
- GitHub release drawer fallback to service-level locate-first anchor windows.
- Digest-tags owner-facing path removal of live manifest scan.
- Edge-proxy-safe SSE heartbeat unification.

## Delivered

- Added `cleanup_inventory_snapshots` and `deploy_check_report_snapshots` persistence paths plus worker orchestration.
- Reworked `POST /api/cleanups/scan` to serve cached snapshot payloads or `pending` envelopes instead of blocking on live Docker scans.
- Reworked cleanup confirm/apply so confirm waits for a fresh snapshot and apply validates the latest fingerprint without re-scanning Docker inline.
- Added `POST /api/deploy-check/report/refresh` and converted `GET /api/deploy-check/report` to cached-read envelopes.
- Parallelized deploy-check local probes and reduced the default local probe timeout to `8s`.
- Removed the Web UI dependency on live `/api/services/{id}/digest-tags`; owner-facing reads now use snapshot semantics.
- Removed Web dependency on `/github-releases/locate`; the drawer now uses unified `release-notes/locate` anchor windows and `direction=older|newer` cursors instead of client-side progressive scans.
- Replaced `15s` SSE keepalive intervals with `5s` heartbeat + immediate keepalive comment on connect.
- Added Storybook coverage for cleanup pending state and deploy-check cached-refreshing / initial-pending states.
- Added the application-level deploy-check gate: startup and foreground resume await a fresh report, required core failures force `/deploy-check`, and the failure page disables Dashboard entry regardless of `neverAutoOpen`.
- Added deterministic mock-only Storybook pass/fail coverage for desktop and `393x852` mobile views; final smoke validation passes all 321 stories.
- Tightened the deploy-check predicate so every required core item must be `pass`; added App-level mock stories proving startup failure redirects remain blocking even when `neverAutoOpen` is true, with 323-story smoke coverage.
- Reconciled historical `missing` discovery records with their linked active Stacks during database startup. The repair applies only the `auto_archive_on_restart` metadata, preserving Compose files, services, and Docker runtime resources so stale paths cannot block deploy-check.
- Extended `scripts/verify_shared_testbox_compose_v2.sh` with a restart regression: it injects the historical state after boot, proves the stale Compose path blocks deploy-check before restart, then verifies startup reconciliation restores the check without changing fixture containers. Kept runs retain a machine-readable `artifacts/summary.json` and `artifacts/remote-test.log` for scoped diagnosis; normal runs remove them with the isolated run directory.

## Verification

- `cargo test -p dockrev-api cleanup -- --nocapture`
- `cargo test -p dockrev-api deploy_check -- --nocapture`
- `cargo test -p dockrev-api startup_reconciles_missing_discovery_projects_with_active_stacks -- --nocapture`
- `scripts/verify_shared_testbox_compose_v2.sh --json-out <evidence-path>` (shared testbox; requires a unique isolated run)
- `cargo test -p dockrev-api github_releases -- --nocapture`
- `cargo test -p dockrev-api snapshot -- --nocapture`
- `bun run --cwd web test`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook -- --url http://127.0.0.1:30080/`
- `cargo test --workspace`, `cargo fmt --all -- --check`, and `cargo clippy --workspace --all-targets --all-features -- -D warnings`

## Notes

- This spec intentionally keeps the mitigation in code instead of depending on EdgeOne console tuning.
- The Storybook visual evidence uses a worktree-owned local daemon on a leased high port for capture, while the regression suite also passed against the existing `30080` Storybook target.
