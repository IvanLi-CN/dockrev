# Implementation

## Scope

- Backend snapshot workers for cleanup inventory and deploy-check report refresh.
- Cleanup API contract migration from synchronous scan to snapshot-backed `ready/pending` reads.
- Deploy-check cached read + async refresh contract.
- Persisted Compose-file reconciliation for stopped discovery projects and their linked Stack records.
- GitHub release drawer fallback to service-level locate-first anchor windows.
- Digest-tags owner-facing path removal of live manifest scan.
- Edge-proxy-safe SSE heartbeat unification.
- Application-level management SSE with bounded in-memory replay and REST-driven invalidation recovery.

## Delivered

- Added `cleanup_inventory_snapshots` and `deploy_check_report_snapshots` persistence paths plus worker orchestration.
- Reworked `POST /api/cleanups/scan` to serve cached snapshot payloads or `pending` envelopes instead of blocking on live Docker scans.
- Reworked cleanup confirm/apply so confirm waits for a fresh snapshot and apply validates the latest fingerprint without re-scanning Docker inline.
- Extended cleanup confirm/apply freshness to a fixed five-minute boundary, added explicit confirm-worker failure responses, and made CleanupPage polling visible, retryable, and stable-label based.
- Added `POST /api/deploy-check/report/refresh` and converted `GET /api/deploy-check/report` to cached-read envelopes.
- Changed the app-level deploy-check gate to immediately accept a cached PASS, request a background recheck, and only block after a newly confirmed failure. No cached report or an existing non-PASS remains blocking.
- Added authenticated `GET /api/events` and `GET /api/events/status`. The event hub uses a per-process generation, `Last-Event-ID` replay, `resync_required`, 100ms entity coalescing, and a 60-second/1024-event in-memory ring without a database event table.
- Published management invalidations from Stack/Service changes, job lifecycle and progress writes, deploy-check reports, cleanup scan terminal states, Discovery scan/archive/restore changes, GHCR configuration/repository/webhook-state and delivery updates, settings writes, and version-inference task state changes.
- Replaced management-page jobs/version/GHCR/cleanup streams and refresh intervals with a single provider connection. Pages preserve stale data during reconnect, defer background-tab REST reads, and fetch only invalidated entities when foregrounded. Service logs and resource monitoring retain their dedicated streams.
- Parallelized deploy-check local probes and reduced the default local probe timeout to `8s`.
- Removed the Web UI dependency on live `/api/services/{id}/digest-tags`; owner-facing reads now use snapshot semantics.
- Removed Web dependency on `/github-releases/locate`; the drawer now uses unified `release-notes/locate` anchor windows and `direction=older|newer` cursors instead of client-side progressive scans.
- Replaced `15s` SSE keepalive intervals with `5s` heartbeat + immediate keepalive comment on connect.
- Management SSE now emits a named, cursor-free heartbeat immediately and every five seconds. A per-tab application transport controller owns the single EventSource, closes stale sessions, rebuilds with bounded `1/2/5/10/15s` backoff, expires silent sessions after 15 seconds, and exposes connection diagnostics and manual retry.
- Management transport recovery is separate from page synchronization: open and foreground resume enqueue one REST resync, protocol-invalid management/heartbeat payloads keep the transport connected while requesting one resync, and service logs/resource streams retain their independent ownership.
- Management transport lifecycle coverage is deterministic: injectable EventSource/scheduler tests cover replacement, fixed backoff, deadlines, late callbacks, protocol-invalid payloads, foreground resume, and disposal; the mock-only recovery story verifies scoped diagnostics and accessible manual retry.
- Owner-approved mock-only visual evidence covers the reconnecting management Alert at desktop and `393x852` mobile viewports; mobile actions use a two-column Alert layout with the retry control anchored to the lower-right edge.
- Added Storybook coverage for cleanup pending state and deploy-check cached-refreshing / initial-pending states.
- Added the application-level deploy-check gate: startup and foreground resume await a fresh report, required core failures force `/deploy-check`, and the failure page disables Dashboard entry regardless of `neverAutoOpen`.
- Added deterministic mock-only Storybook pass/fail coverage for desktop and `393x852` mobile views; final smoke validation passes all 321 stories.
- Tightened the deploy-check predicate so every required core item must be `pass`; added App-level mock stories proving startup failure redirects remain blocking even when `neverAutoOpen` is true, with 323-story smoke coverage.
- Replaced database-startup auto-archiving with a post-start safe discovery scan. Saved Compose files now classify unobserved projects as `stopped`, `missing`, or `invalid`; only every-file `ENOENT` receives `auto_archive_compose_files_missing`.
- Restricted automatic restoration to `auto_archive_compose_files_missing` and the legacy `auto_archive_on_restart` reason. The reconciliation preserves Compose files, services, Docker runtime resources, and every user archive.
- Updated the shared testbox Compose regression to verify stopped, missing, invalid, historical auto-archive recovery, manual archive protection, and deploy-check behavior within one isolated run.

## Verification

- `cargo test -p dockrev-api cleanup -- --nocapture`
- `cargo test -p dockrev-api cleanup_confirm_`
- `cargo test -p dockrev-api deploy_check -- --nocapture`
- `cargo test -p dockrev-api discovery -- --nocapture`
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
