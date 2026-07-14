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

By design. Sensitive values are masked on reads, such as `******` for PATs or dot masks for some keys.

## When should I set `DOCKREV_IMAGE_REPO`?

Set it when the UI needs to identify the Dockrev service accurately for self-upgrade entry.

## Should `docs/plan` and `docs/specs` be user docs?

No. Those directories are engineering artifacts, not end-user documentation.

## Can docs be deployed automatically?

Yes. The `docs-pages` workflow publishes the docs root, the [public Demo](/demo/index.html), and [Storybook](/storybook.html) together.

## What is the difference between Demo and Storybook?

- [Public Demo](/demo/index.html) is the `/demo/` product surface: real app routes, seeded mock state, shareable deep links, and interactive fake writes.
- [Storybook](/storybook.html) is the QA/component/page-state gallery and is not the public product demo.
