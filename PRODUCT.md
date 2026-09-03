# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Dockrev is for self-hosting operators, homelab maintainers, and small infrastructure owners who manage Docker Compose services directly. They use it during routine maintenance, update review, incident recovery, and quick checks from a trusted admin surface.

## Product Purpose

Dockrev helps operators discover Docker Compose services, inspect update and resource state, preview changes, apply updates, recover from failures, and reach service launch targets without turning maintenance into a manual checklist. Success means the operator can scan, decide, act, and verify from one calm interface.

## Positioning

Dockrev is a self-hosted Docker and Docker Compose update manager that combines service discovery, update and lifecycle controls, and operational feedback in a single authenticated web application. It is designed for direct operation of an owner's own infrastructure rather than a hosted fleet-management service.

## Operating Context

The product is used as an authenticated operational console on desktop and mobile web. Typical work includes scanning discovered Compose stacks, reviewing candidate image changes and resource state, previewing or applying updates, starting or stopping services, recovering from failures, and checking the resulting job and lifecycle state. These maintenance actions carry operational risk, so confirmations and failures must remain explicit alongside their resulting job and lifecycle state.

## Capabilities and Constraints

- Discovers Docker Compose projects from running containers and their Compose labels.
- Presents Overview, Services, service-detail, settings, job, update, and recovery workflows in the web application.
- Supports scans, dry-run previews, scoped updates, rollback where available, and guarded lifecycle actions.
- Uses the Docker Engine API for lifecycle observation and resource monitoring; other operational paths use the Docker CLI.
- Runs against the owner's Docker environment and Compose files; discovery and write paths expose actionable errors when required mounted paths or Compose V2 are unavailable.
- The Dockrev service itself uses its separate supervisor console for self-upgrade.

## Brand Commitments

Dockrev is calm, exact, and operationally confident. It must keep state honest, put actions near the evidence that justifies them, and remain compact enough for repeated maintenance work. The interface must not fabricate metrics, health, availability, or recovery outcomes.

## Evidence on Hand

- Product overview, architecture, runtime configuration, and operating workflows: `README.md`.
- Runnable React product surface and theme tokens: `web/src/`.
- Product demo, Storybook, and documentation-site entry points: `README.md` and `docs-site/`.
- Product identity assets: `docs/branding/generated/`.

## Product Principles

- Scan first: the first read should reveal service grouping, state, and action priority.
- Keep state honest: represent real, stale, disabled, unknown, and failed states without pretending.
- Put actions near evidence: update, scan, recovery, and navigation controls live beside the facts that justify them.
- Stay compact and repeatable: controls support repeated operational work without oversized chrome.
- Preserve safe operation: lifecycle and update actions make meaningful constraints, confirmations, and failures visible.

## Accessibility & Inclusion

Dockrev targets WCAG AA contrast for core text and controls, visible keyboard focus, color plus text for operational status, reduced-motion support, and usable layouts from 320px-wide mobile screens through dense desktop dashboards.
