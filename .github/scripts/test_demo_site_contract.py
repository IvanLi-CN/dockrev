#!/usr/bin/env python3
"""Static contract checks for the standalone demo site publication."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


vite = read("web/vite.config.ts")
assert "__DOCKREV_BUILD_INFO__" in vite
assert "build-info.json" in vite
assert "edgeone.json" in vite
assert "variant === 'demo' ? 'dist-demo' : 'dist'" in vite
assert "DOCKREV_DEMO_BASE" in vite
assert "pwaEnabled" in vite and "variant !== 'demo'" in vite

web_package = json.loads(read("web/package.json"))
scripts = web_package["scripts"]
assert "build:demo:pages" not in scripts, "the /demo/ build variant is retired"
assert scripts["build:demo"] == "bun --bun ./node_modules/.bin/tsc -b && bun --bun ./node_modules/.bin/vite build --mode demo"
assert "demoSiteSmoke" not in scripts
assert scripts["test:demo-site"] == "node ./scripts/demo-site-smoke.mjs dist-demo"
assert scripts["e2e"] == "playwright test"

main_tsx = read("web/src/main.tsx")
assert "/api/version" not in main_tsx
assert "startDemoRuntime" in main_tsx
assert "renderDemoBootFailure" in main_tsx
assert main_tsx.index("await startDemoRuntime()") < main_tsx.index("createRoot(document")

build_info = read("web/src/buildInfo.ts")
assert "UNKNOWN_BUILD_VERSION" in build_info
assert "releaseTagForVersion" in build_info

shell = read("web/src/Shell.tsx")
assert "isDemoBundle()" in shell
assert "dockrevBuildInfo().version" in shell

install = read("web/src/mock/dockrevMockApi/install.ts")
assert "mockVersionResponse" in install
assert "0.5.0" not in install, "mock handlers must not hardcode a fake version"

# Demo runtime must be mock-only and self-proving.
bootstrap = read("web/src/demo/runtimeBootstrap.ts")
assert "globalThis.fetch === realFetch" in bootstrap
assert "__DOCKREV_MOCK_DEBUG__" in bootstrap
assert "installDockrevMockApi" in bootstrap

scenes = read("web/src/demo/scenes.ts")
for token in ("'demo'", "d_persona", "d_connection", "d_delay", "d_delay_ms", "d_motion", "d_case", "d_events", "d_cleanup", "d_controls"):
    assert token in scenes, token

# Dockrev has one authorization outcome, so the demo must expose exactly the two
# real states and must not invent role tiers.
demo_types = read("web/src/demo/types.ts")
assert "export type DemoPersonaId = 'allowed' | 'blocked'" in demo_types
for retired in ("'guest'", "'user'", "'admin'"):
    assert f"DemoPersonaId = {retired}" not in demo_types, retired
assert "LEGACY_PERSONA_MAP" in demo_types, "old share links must keep working"

# The mock must enforce the same gate as the backend, before any route handling.
mock_install = read("web/src/mock/dockrevMockApi/install.ts")
assert "isPublicMockApiPath" in mock_install
assert "'auth_required'" in mock_install, "blocked sessions must get the product's 401 code"
assert "authz?.state === 'blocked'" in mock_install

# Public exemptions must match the backend's anonymous surface.
assert "urlPath === '/api/health'" in mock_install
assert "urlPath === '/api/version'" in mock_install
assert "'/api/webhooks/'" in mock_install

inspector = read("web/src/demo/DemoInspector.tsx")
for token in ("全局环境", "模拟鉴权", "网络连接", "请求延迟", "产品主题", "高级", "Recent mutations", "分享与记录"):
    assert token in inspector, token
assert "window.location.reload" not in inspector
assert "setThemePreference" in inspector, "the inspector must drive the product theme preference"
assert "navigateToHref" in read("web/src/demo/store.ts")

# Offline/latency must be real behaviour, not URL decoration.
bootstrap = read("web/src/demo/runtimeBootstrap.ts")
assert "Failed to fetch" in bootstrap, "offline must fail like a browser, not with a fake status"
assert "demoRequestDelayMs" in bootstrap
assert "notifyMutation" in bootstrap, "simulated writes must notify the inspector"

# The demo dev server must serve deep links; `mpa` would 404 every shared URL.
vite = read("web/vite.config.ts")
assert "appType: variant === 'demo' ? 'spa' : 'mpa'" in vite, (
    "the demo build is an SPA and must not use the live app's mpa appType"
)

# A rebuilt mock model must remount the app tree so the product's own boot reads
# (settings/auth) re-run; otherwise the identity chip stays stale.
main_src = read("web/src/main.tsx")
assert "DemoRuntimeBoundary" in main_src
assert "runtime.revision" in main_src, "the boundary must remount on each model rebuild"

# Concurrent identity reads must share one request instead of returning null.
app_src = read("web/src/App.tsx")
assert "authIdentityPromiseRef" in app_src, "auth refresh must share its in-flight promise"

# Scene switches reset scene-owned state but never the reviewer's environment.
scenes_src = read("web/src/demo/scenes.ts")
assert "withShareStatePatch" in scenes_src
store_src = read("web/src/demo/store.ts")
assert "MODEL_KEYS" in store_src and "applyDemoEnvironment" in store_src
assert "'persona'" in store_src, "identity changes must rebuild the model so the product auth reflects them"
assert "recovered" in store_src, (
    "restoring connectivity must re-read the state that failed offline"
)
assert "persona" in store_src.split("MODEL_KEYS")[1].split("] as const")[0], (
    "an authorization change must rebuild the model so the 401 gate applies"
)
assert "previous.connection === 'offline' && next.connection === 'online'" in store_src

# Release is the only publisher, and the artifact identity is checked first.
release = read(".github/workflows/release.yml")
assert "name: Publish the public demo site" in release
assert "publish-demo" in release
assert "needs: [preflight, publish]" in release
assert "DOCKREV_BUILD_CHANNEL: release" in release
assert "vars.DOCKREV_DEMO_PUBLISH_ENABLED == 'true'" in release
assert "deploy-demo-edgeone.sh" in release
assert "dockrev-demo-web-${{ env.VERSION }}" in release
assert release.index("name: Publish the public demo site") < release.index("name: Record release failure context")
assert "needs.publish-demo.result == 'failure'" in release

deploy_script = read(".github/scripts/deploy-demo-edgeone.sh")
assert "DOCKREV_DEMO_EDGEONE_PROJECT" in deploy_script
assert "deployment_ambiguous" in deploy_script
assert "credential_rejected" in deploy_script
assert "demo artifact version" in deploy_script

ci_pr = read(".github/workflows/ci-pr.yml")
assert "name: Demo site smoke" in ci_pr
assert "build:demo:pages" not in ci_pr
assert "demo-site-smoke.mjs web/dist-demo" in ci_pr
assert "! grep -q '/demo/' web/dist-demo/index.html" in ci_pr
assert "DOCKREV_BUILD_CHANNEL: pr" in ci_pr

docs_pages = read(".github/workflows/docs-pages.yml")
assert "build-demo" not in docs_pages

quality = json.loads(read(".github/quality-gates.json"))
assert "Demo site smoke" in quality["informational_checks"]
assert any(
    workflow["workflow"] == "CI (PR)" and "Demo site smoke" in workflow["jobs"]
    for workflow in quality["expected_pr_workflows"]
), "Demo site smoke must be declared in the CI (PR) workflow contract"

print("PASS: standalone demo site publication contract")
