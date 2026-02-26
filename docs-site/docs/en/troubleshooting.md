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
- Any self-upgrade override file path is mounted consistently

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
