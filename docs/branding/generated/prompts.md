# Dockrev Generated Brand Asset Prompts

Generated for Dockrev brand refresh candidates. Direction: marketing-forward product visuals with the existing dark operations-console identity.

Shared brand constraints:
- Product: Dockrev
- Description: Self-hosted Docker/Compose update manager
- Visual language: dark operational console, cyan signal light, sparse green/amber/rose status accents, precise self-hosted infrastructure mood
- Avoid: official Docker or GitHub endorsement, watermarks, fake third-party logos, crowded text, illegible typography

## dockrev-icon-candidate.png
Use case: logo-brand
Asset type: project icon candidate
Primary request: A polished square app icon for Dockrev, a self-hosted Docker/Compose update manager. Create a distinctive brand mark that suggests container stacks, update rotation, version control, and a verified apply action without copying any official Docker logo.
Scene/backdrop: deep blue-black operations console backdrop with subtle grid traces and a controlled cyan glow
Subject: compact 3D/2.5D container cube or stacked service blocks, surrounded by a precise update loop and a small verification signal
Style/medium: premium marketing-grade 3D vector-like icon, crisp edges, high contrast, app-store quality
Composition/framing: centered square composition, generous padding, readable at small sizes, no text
Lighting/mood: cool cyan rim light, quiet professional ops-console mood
Color palette: #061227, #040b1a, #36bffa, #5ccff9, small #22c55e success accent
Materials/textures: matte dark panels, luminous cyan edges, subtle glass/metal highlights
Constraints: no text, no watermark, no official Docker whale, no GitHub logo, no clutter, no tiny unreadable elements

## dockrev-logo-candidate.png
Use case: logo-brand
Asset type: clean logo candidate
Primary request: A clean horizontal Dockrev logo lockup on a dark operations-console background. The mark should be a simplified version of the Dockrev icon concept, followed by the word Dockrev rendered exactly.
Scene/backdrop: plain deep blue-black background, minimal cyan trace lines
Subject: left-side abstract container/update/verify mark plus exact wordmark
Style/medium: polished vector-like brand logo, professional open-source infrastructure product identity
Composition/framing: horizontal lockup, mark on left, Dockrev wordmark on right, centered vertically with generous safe area
Lighting/mood: restrained cyan glow, confident and technical
Color palette: dark navy background, soft ice text, signal cyan accents
Text (verbatim): "Dockrev"
Constraints: spell Dockrev exactly, no tagline, no watermark, no fake vendor logos, no extra words, keep typography clean and legible

## dockrev-product-poster.png
Use case: ads-marketing
Asset type: single product poster
Primary request: A marketing product poster for Dockrev, showing a cinematic self-hosted operations console that manages Docker Compose updates. It should feel like a product launch image for an infrastructure tool, with a realistic dark dashboard scene and clear product identity.
Scene/backdrop: dim workstation / abstract ops room with a large dark dashboard interface showing service cards, update queues, version diffs, health status, and deployment signals
Subject: Dockrev product UI as the hero, surrounded by subtle container stack motifs and update-flow lines
Style/medium: high-end product marketing render, realistic UI mockup plus polished 3D infrastructure accents
Composition/framing: portrait poster, strong central dashboard, headline area near top, visual depth below, enough margin for text
Lighting/mood: dramatic but controlled cyan lighting, confident maintenance-window atmosphere
Color palette: deep blue-black, signal cyan, soft ice text, small green/amber/rose status accents
Text (verbatim): "Dockrev" and "Self-hosted Docker/Compose update manager"
Constraints: spell all text exactly, keep text sparse and readable, no watermark, no official Docker whale, no GitHub logo, no random extra labels beyond tiny abstract UI glyphs

## dockrev-github-social-preview.png
Use case: ads-marketing
Asset type: GitHub Social preview image
Primary request: A GitHub Social preview image for Dockrev, a self-hosted Docker/Compose update manager. Make it marketing-forward but still credible for an open-source infrastructure repo.
Scene/backdrop: wide dark operations dashboard with container stacks, update arrows, release/version signals, and a calm technical atmosphere
Subject: Dockrev name and tagline beside a polished abstract product UI preview
Style/medium: premium open-source product social card, crisp UI-render aesthetic, high contrast at thumbnail size
Composition/framing: wide 1280x640 social card, left/center title block, right-side dashboard/product visual, large safe margins
Lighting/mood: cyan signal glow, precise and trustworthy
Color palette: #061227, #040b1a, #36bffa, #5ccff9, #22c55e, #f59e0b, #ef476f, #e8f1ff
Text (verbatim): "Dockrev" and "Self-hosted Docker/Compose update manager"
Constraints: spell all text exactly, no watermark, no official Docker whale, no GitHub logo, no random extra text, readable when scaled down

## Generation notes

- Mode: `$cvm-imagegen` CLI using `CVM_API_KEY` and `gpt-image-2`.
- Final local post-processing: Pillow standardization and exact text overlay.
- `dockrev-github-social-preview.png` was locally typeset after generation to ensure exact readable copy: `Dockrev` and `Self-hosted Docker/Compose update manager`.
- Direct CVM generation for `dockrev-product-poster.png` failed twice with `503 no healthy pool account is available`; the final poster is a deterministic local composition using the generated icon/social visual, with exact local text rendering.
- Project entry assets are generated from the approved canonical vector source, including the Web favicon family, brand mark, docs logo assets, social preview images, and product poster.

## Final assets

- `dockrev-icon-candidate.png` — 2048x2048 PNG
- `dockrev-logo-candidate.png` — 2048x1024 PNG
- `dockrev-product-poster.png` — 1440x2070 PNG; Web/Docs delivery copy is 1024x1472 PNG
- `dockrev-github-social-preview.png` — 1280x640 PNG

## Flat icon revision

- `dockrev-icon-candidate.png` was revised after owner feedback: the icon must be flat.
- Final icon is a deterministic local flat/vector-style composition using simple geometry: rounded app tile, container stack blocks, update-loop arrows, and verification badge.
- The revised icon intentionally avoids 3D perspective, bevelled material rendering, realistic shadows, and photoreal texture.

## Built-in imagegen logo revision

- Owner requested that the logo must be generated with `$imagegen`.
- Mode: built-in `image_gen` tool, not CVM CLI and not local drawing.
- Output copied from Codex generated-images storage to `dockrev-logo-imagegen-candidate.png`.
- Prompt summary: flat vector-style Dockrev logo lockup, abstract container/update/verified-deployment mark, exact `Dockrev` wordmark, deep navy/cyan palette, no 3D, no bevels, no official Docker/GitHub marks, no tagline.

## Built-in imagegen icon revision

- Owner confirmed the built-in `$imagegen` logo direction and asked whether the icon should also be regenerated.
- Mode: built-in `image_gen` tool, not CVM CLI and not local drawing.
- Output copied from Codex generated-images storage to `dockrev-icon-imagegen-candidate.png`.
- Prompt summary: flat vector-style Dockrev app icon matching the accepted logo direction; abstract container/update/verified-deployment mark; no text; deep navy/cyan palette; no 3D, no bevels, no official Docker/GitHub marks.

## Built-in imagegen poster/social revision

- Owner accepted the built-in `$imagegen` logo/icon direction and asked about the remaining two images.
- Mode: built-in `image_gen` tool, not CVM CLI and not local drawing.
- Outputs copied from Codex generated-images storage to:
  - `dockrev-product-poster-imagegen-candidate.png`
  - `dockrev-github-social-preview-imagegen-candidate.png`
- Prompt summary: flat/semi-flat Dockrev product marketing visuals matching the accepted logo/icon direction; exact `Dockrev` and `Self-hosted Docker/Compose update manager` copy; dark ops dashboard; no official Docker/GitHub marks.

## Canonical vector assets

- `docs/branding/dockrev-icon-source.svg` is the canonical square icon geometry.
- `docs/branding/dockrev-logo-source.svg` is the canonical horizontal dark-theme lockup generated from the icon geometry and outlined wordmark glyphs.
- `docs/branding/generate_brand_assets.py` derives transparent marks, dark/light horizontal lockups, PNG sizes, PWA icons, Apple Touch icons, multi-size ICO files, marketing images, and the final asset contact sheet without raster tracing.
- Files ending in `-imagegen-candidate.png` are retained only as historical generation references. Project-facing and generic `*-candidate.png` outputs are regenerated from the canonical vector geometry.
- Web and docs horizontal assets include dark and light variants:
  - dark mark: `#20b8ff` through `#1cb4fb`; check: `#16d563` through `#10cd5c`; wordmark: `#e8f1ff`
  - light mark: `#0d86dd` through `#086cba`; check: `#16934e` through `#138347`; wordmark: `#102842`
- App icons and favicons retain the canonical deep-ops background in every theme so the product identity remains stable outside the application shell.
