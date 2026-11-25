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
for token in ("'demo'", "d_persona", "d_net", "d_case", "d_events", "d_cleanup", "d_controls"):
    assert token in scenes, token

inspector = read("web/src/demo/DemoInspector.tsx")
for token in ("scene", "persona", "network", "模拟写入", "分享"):
    assert token in inspector, token
assert "window.location.reload" not in inspector
assert "navigateToHref" in read("web/src/demo/store.ts")

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
