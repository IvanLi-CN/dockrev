# Oversized file inventory (after)

- Budget check command: `python3 /Users/ivan/Projects/Ivan/dockrev/.github/scripts/check-file-budgets.py`
- Result: `OK: no tracked source files exceed the configured line budgets.`
- Total oversized tracked files in governed scope: 0

| Area | Baseline file | Current entry / replacement | Current LOC | Budget | Result |
| --- | --- | --- | ---: | ---: | --- |
| backend route/db | `crates/dockrev-api/src/api/github_packages.rs` | `crates/dockrev-api/src/api/github_packages.rs` | 995 | 1500 | OK |
| backend route/db | `crates/dockrev-api/src/api/operations.rs` | `crates/dockrev-api/src/api/operations.rs` | 1388 | 1500 | OK |
| backend route/db | `crates/dockrev-api/src/api/services.rs` | `crates/dockrev-api/src/api/services.rs` | 1153 | 1500 | OK |
| tests | `crates/dockrev-api/src/api/tests.rs` | `crates/dockrev-api/src/api/tests/mod.rs` | 48 | 1500 | OK |
| backend runtime | `crates/dockrev-api/src/cleanup.rs` | `crates/dockrev-api/src/cleanup.rs` | 1436 | 1500 | OK |
| backend route/db | `crates/dockrev-api/src/db/mod.rs` | `crates/dockrev-api/src/db/mod.rs` | 567 | 1500 | OK |
| backend route/db | `crates/dockrev-api/src/db/new_version_discoveries.rs` | `crates/dockrev-api/src/db/new_version_discoveries.rs` | 848 | 1500 | OK |
| backend runtime | `crates/dockrev-api/src/discovery.rs` | `crates/dockrev-api/src/discovery.rs` | 1244 | 1500 | OK |
| backend runtime | `crates/dockrev-api/src/ghcr_webhook_jobs.rs` | `crates/dockrev-api/src/ghcr_webhook_jobs.rs` | 1446 | 1500 | OK |
| backend runtime | `crates/dockrev-api/src/notify.rs` | `crates/dockrev-api/src/notify.rs` | 981 | 1500 | OK |
| backend runtime | `crates/dockrev-api/src/registry.rs` | `crates/dockrev-api/src/registry.rs` | 1475 | 1500 | OK |
| backend runtime | `crates/dockrev-api/src/updater.rs` | `crates/dockrev-api/src/updater.rs` | 1378 | 1500 | OK |
| backend runtime | `crates/dockrev-supervisor/src/app/ui.rs` | `crates/dockrev-supervisor/src/app/ui.rs` | 942 | 1500 | OK |
| frontend transport/mock | `web/src/api.ts` | `web/src/api.ts` | 798 | 1200 | OK |
| frontend pages | `web/src/pages/OverviewPage.tsx` | `web/src/pages/OverviewPage.tsx` | 893 | 1200 | OK |
| frontend pages | `web/src/pages/ServiceDetailPage.tsx` | `web/src/pages/ServiceDetailPage.tsx` | 466 | 1200 | OK |
| frontend pages | `web/src/pages/ServicesPage.tsx` | `web/src/pages/ServicesPage.tsx` | 805 | 1200 | OK |
| frontend pages | `web/src/pages/SettingsPage.tsx` | `web/src/pages/SettingsPage.tsx` | 1061 | 1200 | OK |
| frontend transport/mock | `web/src/stories/mocks/dockrevMockApi.ts` | `web/src/stories/mocks/dockrevMockApi.ts` | 6 | 1200 | OK |
