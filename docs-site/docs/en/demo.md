---
title: Public Demo
description: Scenes, share parameters and version semantics of the standalone Dockrev demo site.
---

# Public Demo

The public demo is a **separate site**: <https://dockrev-demo.ivanli.cc/>. It is not a subdirectory of the docs site, and it never talks to a real Dockrev backend.

## What it is

- Reuses the real React app pages, routes and state model, with data served by an in-browser mock layer.
- Writes only mutate the in-memory model and are recorded in the inspector's simulated-write list.
- The version is fixed at build time: the UI shows the version the artifact was built from, and `/api/version` reads back the same value. No live version endpoint is queried.

## Scene parameters

| Parameter | Purpose | Values |
| --- | --- | --- |
| `demo` | Scene selection | `overview`, `services`, `service-actions`, `service-versions`, `queue`, `queue-logs`, `settings`, `ghcr-registry`, `cleanup`, `deploy-check` |
| `d_persona` | Demo persona | `anonymous`, `admin` |
| `d_net` | Network behaviour | `normal`, `slow`, `faulty` |
| `d_case` | Async data state | `default`, `cold`, `cache-refresh`, `error`, `job-detail-retry` |
| `d_events` | Management event stream | `default`, `reconnecting` |
| `d_cleanup` | Cleanup scenario | `cleanup-console-storage-normal`, `cleanup-console-confirm-pending`, `cleanup-console-confirm-failed` |
| `d_controls` | Hide the inspector | `hidden` |

Defaults are omitted from the URL. `https://dockrev-demo.ivanli.cc/services?demo=services&d_net=slow` records only the non-default `d_net`.

## Demo inspector

- Desktop: a floating panel docked left or right, collapsible to a `DEMO` bubble, with per-scene position memory.
- Mobile: a `DEMO` bubble that opens a bottom drawer.
- The panel offers scene switching, persona, network, data, a share link and seed reset. Scene switching is client-side navigation and never reloads the document.
- Use `d_controls=hidden` for clean screenshots.

## Relationship to Storybook

Storybook is the component and page-state QA gallery; it does not replace the product demo surface. The demo exposes clickable, shareable real product paths.

## Publication and configuration

- Only `Release` builds and publishes the demo site; pull requests only build and smoke it.
- Enabling publication requires an owner-side EdgeOne Makers direct-upload project and domain binding, plus the repository variable `DOCKREV_DEMO_PUBLISH_ENABLED=true` and the secrets `EDGEONE_API_TOKEN` and `DOCKREV_DEMO_EDGEONE_PROJECT`. While disabled, `Release` still produces the `dockrev-demo-web-<version>` artifact for a manual deployment.
