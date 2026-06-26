# Implementation

## Scope

- Backend snapshot workers for cleanup inventory and deploy-check report refresh.
- Cleanup API contract migration from synchronous scan to snapshot-backed `ready/pending` reads.
- Deploy-check cached read + async refresh contract.
- GitHub release drawer fallback to paginated client-side locate.
- Digest-tags owner-facing path removal of live manifest scan.
- Edge-proxy-safe SSE heartbeat unification.

## Delivered

- Added `cleanup_inventory_snapshots` and `deploy_check_report_snapshots` persistence paths plus worker orchestration.
- Reworked `POST /api/cleanups/scan` to serve cached snapshot payloads or `pending` envelopes instead of blocking on live Docker scans.
- Reworked cleanup confirm/apply so confirm waits for a fresh snapshot and apply validates the latest fingerprint without re-scanning Docker inline.
- Added `POST /api/deploy-check/report/refresh` and converted `GET /api/deploy-check/report` to cached-read envelopes.
- Parallelized deploy-check local probes and reduced the default local probe timeout to `8s`.
- Removed the Web UI dependency on live `/api/services/{id}/digest-tags`; owner-facing reads now use snapshot semantics.
- Removed Web dependency on `/github-releases/locate`; the drawer progressively searches paginated release results on the client.
- Replaced `15s` SSE keepalive intervals with `5s` heartbeat + immediate keepalive comment on connect.
- Added Storybook coverage for cleanup pending state and deploy-check cached-refreshing / initial-pending states.

## Verification

- `cargo test -p dockrev-api cleanup -- --nocapture`
- `cargo test -p dockrev-api deploy_check -- --nocapture`
- `cargo test -p dockrev-api github_releases -- --nocapture`
- `cargo test -p dockrev-api snapshot -- --nocapture`
- `bun run --cwd web test`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook -- --url http://127.0.0.1:30080/`

## Notes

- This spec intentionally keeps the mitigation in code instead of depending on EdgeOne console tuning.
- The Storybook visual evidence uses a worktree-owned local daemon on a leased high port for capture, while the regression suite also passed against the existing `30080` Storybook target.
