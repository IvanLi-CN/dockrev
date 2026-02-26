---
title: FAQ
description: Frequently asked questions and best practices.
---

# FAQ

## What deployment model does Dockrev support?

Dockrev primarily targets Docker Compose projects discovered via Compose labels.

## Is GHCR required?

No. GHCR webhook is optional; Dockrev can still work with regular registry checks.

## Why is PAT never returned in plain text?

By design. Sensitive values are masked as `******` on reads.

## When should I set `DOCKREV_IMAGE_REPO`?

Set it when the UI needs to identify the Dockrev service accurately for self-upgrade entry.

## Should `docs/plan` and `docs/specs` be user docs?

No. Those directories are engineering artifacts, not end-user documentation.

## Can docs be deployed automatically?

Yes. Use the `docs-pages` workflow for GitHub Pages deployment.
