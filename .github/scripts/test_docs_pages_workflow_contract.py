#!/usr/bin/env python3
"""Static contract checks for retry-safe GitHub Pages deployment artifacts.

The public demo is a separate EdgeOne site, so the docs Pages workflow must not
build, assemble, or smoke it.
"""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
workflow = (ROOT / ".github/workflows/docs-pages.yml").read_text(encoding="utf-8")

artifact_name = "github-pages-${{ github.run_id }}-${{ github.run_attempt }}"
artifact_expression = "${{ env.PAGES_ARTIFACT_NAME }}"

assert workflow.count("uses: actions/upload-pages-artifact@v3") == 1
assert workflow.count("uses: actions/deploy-pages@v4") == 1
assert f"PAGES_ARTIFACT_NAME: {artifact_name}" in workflow
assert "pages: write" in workflow
assert "id-token: write" in workflow

upload_start = workflow.index("      - name: Upload Pages artifact")
deploy_start = workflow.index("      - name: Deploy to GitHub Pages")
upload_section = workflow[upload_start:deploy_start]
deploy_section = workflow[deploy_start:]

assert f"name: {artifact_expression}" in upload_section
assert f"artifact_name: {artifact_expression}" in deploy_section

assert "build:demo" not in workflow, "docs Pages must not build the demo site"
assert "assemble-pages-site.sh .tmp/docs-site .tmp/storybook-static .tmp/web-demo" not in workflow
assert "demo-pages-smoke.mjs" not in workflow

assembly = (ROOT / ".github/scripts/assemble-pages-site.sh").read_text(encoding="utf-8")
assert "<demo_dir>" not in assembly, "assembly script must not accept a demo dir"
assert "mkdir -p \"$output_dir/storybook\" \"$output_dir/demo\"" not in assembly
assert 'if [[ -e "$output_dir/demo" ]]' in assembly

print("PASS: Docs Pages retry artifact contract (demo site is separate)")
