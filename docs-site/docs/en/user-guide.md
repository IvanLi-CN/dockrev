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

### 5) Track jobs in Queue

- Monitor states: `running/success/failed`
- Open logs per job for diagnosis

## Version inference

When tags are not strict semver, Dockrev displays digest-based inferred tags.

![Version inference](../assets/version-inference.png)

## Self-upgrade flow

- For Dockrev service itself, UI provides an “Upgrade Dockrev” entry
- Availability depends on `GET /supervisor/self-upgrade` and auth forwarding
