---
name: Dockrev
description: A calm, data-dense self-hosted Docker and Compose update manager.
colors:
  deep-ops-bg: "#061227"
  night-base: "#040b1a"
  ink-panel: "#0d2342"
  midnight-panel: "#07142a"
  deep-ink: "#011222"
  primary-cyan: "#36bffa"
  secondary-cyan: "#5ccff9"
  success-green: "#22c55e"
  warning-amber: "#f59e0b"
  error-rose: "#ef476f"
  text-primary: "#e8f1fff5"
  text-solid: "#e8f1ff"
  text-muted: "#dceafed1"
  text-dim: "#9eb2cce0"
  border-blue: "#9cc0e83b"
  surface-wash: "#ffffff0c"
  surface-strong: "#ffffff15"
  focus-cyan: "#36bffa70"
typography:
  display:
    fontFamily: "Avenir Next, Avenir, Segoe UI, PingFang SC, Hiragino Sans GB, Microsoft YaHei, Noto Sans SC, system-ui, -apple-system, sans-serif"
    fontSize: "22px"
    fontWeight: 750
    lineHeight: 1.18
    letterSpacing: "0"
  headline:
    fontFamily: "Avenir Next, Avenir, Segoe UI, PingFang SC, Hiragino Sans GB, Microsoft YaHei, Noto Sans SC, system-ui, -apple-system, sans-serif"
    fontSize: "21px"
    fontWeight: 650
    lineHeight: 1.25
    letterSpacing: "0"
  title:
    fontFamily: "Avenir Next, Avenir, Segoe UI, PingFang SC, Hiragino Sans GB, Microsoft YaHei, Noto Sans SC, system-ui, -apple-system, sans-serif"
    fontSize: "15px"
    fontWeight: 640
    lineHeight: 1.25
    letterSpacing: "0"
  body:
    fontFamily: "Avenir Next, Avenir, Segoe UI, PingFang SC, Hiragino Sans GB, Microsoft YaHei, Noto Sans SC, system-ui, -apple-system, sans-serif"
    fontSize: "12px"
    fontWeight: 440
    lineHeight: 1.5
    letterSpacing: "0"
  label:
    fontFamily: "Avenir Next, Avenir, Segoe UI, PingFang SC, Hiragino Sans GB, Microsoft YaHei, Noto Sans SC, system-ui, -apple-system, sans-serif"
    fontSize: "11px"
    fontWeight: 650
    lineHeight: 1.2
    letterSpacing: "0"
  mono:
    fontFamily: "JetBrains Mono, Fira Code, SFMono-Regular, Menlo, Monaco, Consolas, Liberation Mono, Courier New, monospace"
    fontSize: "11px"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "0"
rounded:
  xs: "3px"
  sm: "5px"
  md: "6px"
  lg: "10px"
  xl: "14px"
  popover: "16px"
  pill: "999px"
spacing:
  xxs: "4px"
  xs: "6px"
  sm: "8px"
  md: "10px"
  lg: "14px"
  xl: "24px"
  page-x: "28px"
  page-y: "48px"
components:
  button-primary:
    backgroundColor: "{colors.primary-cyan}"
    textColor: "{colors.deep-ink}"
    rounded: "{rounded.lg}"
    padding: "0 13px"
    height: "32px"
  button-ghost:
    backgroundColor: "{colors.surface-wash}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.lg}"
    padding: "0 13px"
    height: "32px"
  button-danger:
    backgroundColor: "{colors.error-rose}"
    textColor: "{colors.deep-ink}"
    rounded: "{rounded.lg}"
    padding: "0 13px"
    height: "32px"
  search-input:
    backgroundColor: "{colors.midnight-panel}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.md}"
    padding: "0 14px"
    height: "36px"
  service-card:
    backgroundColor: "{colors.ink-panel}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.md}"
    padding: "12px 9px 9px"
  metric-cell:
    backgroundColor: "{colors.midnight-panel}"
    textColor: "{colors.text-muted}"
    rounded: "{rounded.sm}"
    padding: "5px 3px"
    height: "48px"
  count-badge:
    backgroundColor: "{colors.midnight-panel}"
    textColor: "{colors.secondary-cyan}"
    rounded: "{rounded.pill}"
    padding: "0 8px"
    height: "21px"
---

# Design System: Dockrev

## 1. Overview

**Creative North Star: "The Quiet Ops Console"**

Dockrev is a product interface for people operating real Docker Compose services under time pressure. The physical scene is an operator checking update and resource state on a dim desktop monitor during a maintenance window, then returning from a phone later to confirm what changed. The design is dark by default because it reduces glare in that scene, but the darkness is functional: it frames dense state, metrics, paths, and actions without theatrical contrast.

The system uses restrained color, compact hierarchy, and small repeated modules. It should feel like a precise control surface, not a landing page, not a generic dark-blue SaaS dashboard, and not a glassy observability poster. Every colored element must carry state, action, identity, or focus.

**Key Characteristics:**
- Compact service navigation, grouped for scanning.
- Real operational state before decoration.
- Cool tinted neutrals with rare cyan action color.
- Small-radius cards and metric cells that stay stable across data changes.
- Familiar admin patterns: top bar, side nav, search, grouped lists, tables, dialogs only when necessary.

## 2. Colors

The palette is a restrained cold-operations system: deep blue-black structure, quiet cyan action, and semantic status colors only where the state needs it.

### Primary

- **Signal Cyan**: the primary action, focus, selected navigation, and important link color. It is rare on a screen; its scarcity makes it meaningful.

### Secondary

- **Cool Cyan Echo**: secondary chart and accent support for metric summaries, count badges, and subtle information emphasis.

### Tertiary

- **Operational Green**: success, running, healthy, completed, and allowed states.
- **Maintenance Amber**: warnings, stale data, pending review, and recoverable attention states.
- **Fault Rose**: destructive actions, failed jobs, blocked service state, and unrecoverable errors.

### Neutral

- **Deep Ops Background**: the page and application shell foundation.
- **Ink Panel**: the default card and modal surface.
- **Midnight Panel**: nested surface for top bars, search wells, metric cells, and subdued controls.
- **Soft Ice Text**: primary and muted text, always tinted toward the blue system.
- **Blue Trace Border**: low-contrast separators and panel outlines.
- **Surface Wash**: quiet hover, chip, table row, and ghost button fill.

### Named Rules

**The Truthful Color Rule.** Cyan, green, amber, and rose are not decoration. They must map to action, focus, health, warning, or error.

**The Restrained Console Rule.** Any single screen should read as neutral first. Accent color should remain sparse enough that the operator can locate state immediately.

## 3. Typography

**Display Font:** Avenir Next with system UI and CJK fallbacks.
**Body Font:** Avenir Next with system UI and CJK fallbacks.
**Label/Mono Font:** JetBrains Mono with platform monospace fallbacks.

**Character:** The typography is compact and product-native. It uses weight and grouping more than large scale, because this is an operational tool, not a brand surface.

### Hierarchy

- **Display** (750, 22px, 1.18): page titles and the largest dashboard headings only.
- **Headline** (650, 21px, 1.25): Homepage-style group headings and major section labels.
- **Title** (640, 15px, 1.25): service card titles, row names, modal titles.
- **Body** (440, 12px, 1.5): descriptions, table cells, helper text, and dense UI copy. Prose blocks should stay under 75ch.
- **Label** (650, 11px, 1.2): metric labels, chips, compact badges, table metadata.
- **Mono** (700, 11px, 1): counts, identifiers, resource values, and technical tokens.

### Named Rules

**The Dense Sans Rule.** Do not introduce display fonts, decorative serif headings, or fluid type. Dockrev should look native to an expert admin console.

**The No Shouting Rule.** Uppercase is reserved for tiny status badges and metric labels. Larger UI text stays sentence case or title case.

## 4. Elevation

Dockrev uses hybrid depth: tonal layering at rest, soft shadow for true floating surfaces, and tiny motion for hover affordance. Service cards are mostly flat panels. Popovers, drawers, modals, and large cards can use ambient shadows when they must separate from the shell.

### Shadow Vocabulary

- **Soft Panel Shadow** (`0 14px 38px rgba(1, 8, 20, 0.44)`): default elevated cards and quiet content panels.
- **Deep Card Shadow** (`0 24px 68px rgba(1, 10, 24, 0.54)`): prominent dashboard panels or focused surfaces.
- **Floating Popover Shadow** (`0 24px 90px rgba(0, 0, 0, 0.42)`): popovers and menus.
- **Mobile Drawer Shadow** (`28px 0 70px rgba(1, 8, 20, 0.5)`): full-height mobile navigation drawer.

### Named Rules

**The Layer Before Shadow Rule.** Prefer background tone, border, and spacing before adding a shadow. Shadows must explain stacking or focus.

**The One-Pixel Trace Rule.** Borders are thin traces. Do not use thick colored side stripes as status decoration.

## 5. Components

### Buttons

- **Shape:** compact rounded rectangles (10px radius), 32px high by default.
- **Primary:** cyan action fill with deep ink text, used for scan, apply, confirm, and other high-intent actions.
- **Ghost:** washed neutral fill, blue trace border, and primary text for secondary operations.
- **Danger:** rose fill for destructive or rollback-risk actions.
- **Hover / Focus:** hover lifts by 1px only. Focus uses a visible cyan ring and never relies on color alone.

### Chips

- **Style:** compact pills or soft rounded tags, 26px to 28px high, neutral surface fill, muted text.
- **State:** active chips may use cyan tint, but inactive chips stay neutral. Count badges use mono numerals.

### Cards / Containers

- **Corner Style:** application cards use 14px to 16px radius; Homepage service cards use tighter 6px radius.
- **Background:** panels use Ink Panel or Midnight Panel with subtle tonal gradients only on larger dashboard cards.
- **Shadow Strategy:** service cards do not need heavy shadow. Popovers and drawers use the shadow vocabulary in Elevation.
- **Border:** one-pixel blue trace, low opacity.
- **Internal Padding:** dense cards use 9px to 12px; large content cards use 14px to 20px.

### Inputs / Fields

- **Style:** 36px search fields, 6px radius, deep panel background, one-pixel blue trace border; Homepage search submits on Enter and does not add a separate search button.
- **Focus:** cyan border plus soft 3px glow.
- **Error / Disabled:** rose border and subtle rose glow for error; reduced opacity and no pointer affordance for disabled.

### Navigation

- **Desktop:** fixed side navigation plus sticky top bar. Active items use cyan-tinted fill and stronger weight.
- **Mobile:** top bar hamburger opens a full-height left drawer. Drawer items are list rows, not cards.
- **Identity:** user identity is a compact chip on desktop and a circular avatar button on narrow screens.

### Homepage Service Launcher

- **Structure:** top resource/search/time strip, then responsive grouped columns. Desktop uses multiple columns; mobile uses one column.
- **Service Card:** icon, title, description, one status badge, and four stable metric cells: CPU, MEM, RX, TX.
- **Search:** search filters group, name, description, image, stack, and service metadata, submits on Enter, and keeps hidden metadata out of the visible card.
- **Metric Cells:** values can show real data, stale state, disabled state, or placeholders. Cell dimensions must not shift when values change.

## 6. Do's and Don'ts

### Do:

- **Do** keep Dockrev compact, dense, and scannable.
- **Do** use real operational data and explicit placeholders for stale, disabled, or missing metrics.
- **Do** reserve Signal Cyan for action, focus, selected state, and meaningful info.
- **Do** keep Homepage service cards tight: 6px radius, four metric cells, one status badge.
- **Do** maintain visible focus rings for every keyboard target.
- **Do** use familiar product patterns before inventing custom interactions.

### Don't:

- **Don't** create a Marketing landing-page hero for authenticated product routes.
- **Don't** use Oversized decorative cards for operational content.
- **Don't** drift into a Generic dark-blue SaaS dashboard; every surface must earn its density and state.
- **Don't** use Glassmorphism as default. Blur is allowed only for overlays that need backdrop separation.
- **Don't** use Neon cyberpunk styling, purple gradients, or decorative glow as identity.
- **Don't** show Fake metrics or decorative status.
- **Don't** use Modal-first flows for routine decisions.
- **Don't** ship Dense but unstructured admin tables.
- **Don't** use thick colored side stripes, gradient text, or repeated identical card grids.
