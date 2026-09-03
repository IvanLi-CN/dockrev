---
title: Quick Start
description: Bring up Dockrev quickly and run the first scan/check cycle.
---

# Quick Start

Goal: get Dockrev running in about 10 minutes and validate the end-to-end flow.

## Prerequisites

- Docker Engine is available
- Docker Compose is available (`docker-compose` or `docker compose`)
- Local port `50883` is available

## Minimal startup

```bash
cd deploy
mkdir -p data
# Dockrev stages this file for update-job Docker/Compose auth, so you do not need an extra /root/.docker/config.json mount.
cp ~/.docker/config.json data/docker-config.json
# If you rely on Docker contexts, configure DOCKREV_DOCKER_CONFIG with a real config.json path instead of a renamed copy.

docker compose up --build
```

Entry points:

- UI: `http://127.0.0.1:50883/`
- API health: `http://127.0.0.1:50883/api/health`
- Supervisor: `http://127.0.0.1:50883/supervisor/`

## Installed app icon updates

Dockrev keeps the Web App Manifest `id`, `scope`, and `start_url` stable for an installation. Manifest regular/maskable icons and browser favicons are published at content-hashed URLs, while the HTML, manifest, and service worker are revalidated. The product page uses the Manifest as its only install-icon metadata source: it neither declares an `apple-touch-icon` link nor publishes a root `apple-touch-icon*.png` fallback. A new build can therefore deliver new icon bytes without changing the installed app identity or relying on a reinstall as the normal update path.

Android Chrome WebAPKs and Chromium desktop PWA installations follow the manifest update lifecycle and its platform-controlled refresh schedule. A browser shortcut that is not a manifest-backed PWA may keep the icon it captured when it was created.

Safari on iOS/iPadOS and existing Web Clips have a separate limitation: an existing Web Clip keeps the icon and metadata saved by the platform, and a website cannot force-migrate it. Dockrev does not restore an `apple-touch-icon` link or root fallback on the product page, and does not claim to update an existing Web Clip. The same applies to browsers without an in-place manifest migration mechanism. This is a platform limitation, not a routine Dockrev update instruction.

After a release, verify the deployed product artifact with an HTML parser and the Web App Manifest: confirm one manifest link, no product `apple-touch-icon` link, unchanged `id`/`scope`/`start_url`, current hashed icon bytes, revalidation headers for metadata, immutable headers for hashed icons, and no manifest/icon entries in `sw.js` precache.

## First validation checklist

1. Open Overview and confirm services are listed.
2. Run Discovery scan from UI.
3. Trigger one Check job for any service.
4. Open Queue and confirm state transitions.

## Local dev startup (without containers)

### Worktree dependencies

```bash
bun run hooks:install
bun run bootstrap:worktree
```

`hooks:install` installs a shared Git `post-checkout` hook. New linked worktrees automatically run the project-local bootstrap: root, `web/`, and `docs-site/` Bun installs plus `cargo fetch --locked`. It does not install Bun, Rust, Playwright browsers, or system packages. Set `DOCKREV_BOOTSTRAP_SKIP=1` to skip the automatic hook run.

### Backend

```bash
DOCKREV_HTTP_ADDR=127.0.0.1:50883 DOCKREV_DB_PATH=/tmp/dockrev.sqlite3 cargo run -p dockrev-api --bin dockrev
```

### Supervisor

```bash
DOCKREV_SUPERVISOR_HTTP_ADDR=127.0.0.1:50884 cargo run -p dockrev-supervisor --bin dockrev-supervisor
```

### Web UI

```bash
cd web
bun install
bun run dev
```

## Next steps

- Continue with [Deployment](./deploy)
- Then apply [Configuration](./config)
