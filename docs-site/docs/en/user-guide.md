---
title: User Guide
description: Daily usage flows and UI operations in Dockrev.
---

# User Guide

## Page map

- `Overview`: global status and bulk actions
- `Services`: service-level view and filtering
- `Service Detail`: single-service check/update/version inference
- `Queue`: job tracking and logs
- `Settings`: notifications, GHCR webhook, system options
- GHCR webhook setup details: [Integrations -> GitHub Packages (GHCR) webhook](/en/integrations#github-packages-ghcr-webhook)

![Services](../assets/services.png)

## Typical daily workflow

### 1) Discovery

- Trigger from Overview/Services
- Refresh compose projects and running services

### 2) Check

- Trigger by service, stack, or all
- Fetch candidate versions and remarks

### 3) Dry-run preview

- Trigger from Service Detail
- Validate expected update behavior before apply

### 4) Apply update

- Trigger from Overview, Services, or Service Detail
- Submit update jobs according to selected scope

### 5) Auto-update policies

- Configure from Service Detail, or open Stack Detail from the Service Detail top actions and use the “Stack Settings” drawer
- Stack policies define defaults; each Service can inherit, override, or disable auto updates
- Rules match candidate versions or tags with semver, regex, or glob
- Delayed rules require both gates: candidate first-seen age and current version lag behind N matching versions
- Automatic apply only runs after scheduled checks or GHCR webhook checks; UI manual scans never auto deploy

### 6) Track jobs in Queue

- Monitor states: `running/success/failed`
- Open logs per job for diagnosis

## Version inference

When tags are not strict semver, Dockrev displays digest-based inferred tags.

![Version inference](../assets/version-inference.png)

## Self-upgrade flow

- For Dockrev service itself, UI provides an “Upgrade Dockrev” entry
- Availability depends on `GET /supervisor/self-upgrade` and auth forwarding
