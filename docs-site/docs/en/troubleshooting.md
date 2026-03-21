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
- Any self-upgrade `dockrev-override-*.yml` temp file path is mounted consistently
- Any user-managed extra compose / override file still exists and is mounted

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

1. If the only unreadable file is a Dockrev-generated `dockrev-override-*.yml` temp file, rerun discovery and let Dockrev fall back to the remaining readable compose files.
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
