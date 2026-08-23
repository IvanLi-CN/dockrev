---
title: Troubleshooting
description: Failure diagnosis paths and recovery actions.
---

# Troubleshooting

## 1) UI/API returns 401

Check:

- Forward auth header is injected correctly
- Anonymous dev mode is disabled/enabled as intended
- Proxy forwarding rules include supervisor paths when needed

## 2) Compose projects are not discovered

Check:

- Compose labels are present on running containers
- `config_files` absolute paths are readable inside dockrev container
- The supervisor state path is mounted consistently: `self-upgrade.override.yml` is next to the configured absolute `DOCKREV_SUPERVISOR_STATE_PATH`, with API read-only and supervisor read-write access.
- Durable update provenance is mounted at `DOCKREV_MANAGED_OVERRIDE_DIR`. A historical `/tmp/dockrev-override-<project>-<ulid>.yml` is disposable history, not a current managed file.
- Any user-managed extra compose / override file still exists and is mounted

If a service is stopped but every saved Compose file is readable and valid, discovery reports `stopped` instead of archiving it. Open the linked Stack and use the existing start action. Dockrev auto-archives only when every saved Compose file is absent. A partial absence, permission/I-O failure, or parse error stays visible as `invalid`; repair the file or mount and rerun discovery. A user archive is never removed by discovery.

## 3) Check jobs fail or run slowly

Check:

- Registry rate limiting (`429`)
- Retry parameters are adequate
- Network connectivity and registry credentials

## 4) GHCR webhook does not trigger scan

Check:

- Callback URL is publicly reachable over HTTPS
- Delivery reaches Dockrev
- `X-Hub-Signature-256` validation passes
- Repo is selected in tracked repo list
- Queue shows matching `check.service` jobs; discovery is only the zero-match fallback

Immediate actions:

1. If the warning is `warning:config_files_stale_dockrev_temp_override`, keep it visible and use the administrator-only reconciliation action. It requires a matching running-image RepoDigest and performs no pull; a rescan alone must not hide it.
2. If the unreadable file is a user-managed compose/override file, fix the same-absolute-path mount first; discovery will keep the project invalid until that file is readable.
3. If webhook deliveries already return `200` but candidates stay stale, inspect the matched `check.service` job logs; digest-only image refs should now be accepted instead of failing as `invalid image ref`.

## 5) Self-upgrade button is disabled

Check:

- `/supervisor/self-upgrade` reachability
- Forward auth header on supervisor routes
- Correct target image repo configuration

## 6) Job appears stuck in running state

Check:

- Scope-level mutex conflicts with other jobs
- Startup recovery behavior after restart
- Per-job logs to identify blocking phase
