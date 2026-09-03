---
name: Dockrev
description: A compact dual-theme control surface for operating self-hosted Docker Compose services.
colors:
  background: "#061227"
  light-background: "#f6faff"
  panel: "#0d2342"
  light-panel: "#ffffff"
  raised-panel: "#07142a"
  light-raised-panel: "#fbfdff"
  primary: "#36bffa"
  primary-highlight: "#8ee7ff"
  primary-deep: "#1767a2"
  light-primary: "#0c79cf"
  secondary: "#5ccff9"
  light-secondary: "#1f93dd"
  info: "#38bdf8"
  light-info: "#0e8ed2"
  success: "#22c55e"
  light-success: "#138347"
  warning: "#f59e0b"
  light-warning: "#a55706"
  error: "#ef476f"
  light-error: "#bf1240"
  text: "rgba(232, 241, 255, 0.96)"
  light-text: "rgba(16, 40, 66, 0.96)"
  muted-text: "rgba(158, 178, 204, 0.88)"
  light-muted-text: "rgba(58, 86, 118, 0.86)"
  border: "rgba(156, 192, 232, 0.23)"
  light-border: "rgba(14, 56, 100, 0.12)"
typography:
  display:
    fontFamily: "'Avenir Next', 'Avenir', 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'Noto Sans SC', system-ui, -apple-system, sans-serif"
    fontSize: "30px"
    fontWeight: 740
    lineHeight: 1.2
    letterSpacing: "-0.02em"
  title:
    fontFamily: "'Avenir Next', 'Avenir', 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'Noto Sans SC', system-ui, -apple-system, sans-serif"
    fontSize: "16px"
    fontWeight: 670
    lineHeight: 1.25
    letterSpacing: "0"
  navigation:
    fontFamily: "'Avenir Next', 'Avenir', 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'Noto Sans SC', system-ui, -apple-system, sans-serif"
    fontSize: "13px"
    fontWeight: 590
    lineHeight: 1.5
    letterSpacing: "0"
  body:
    fontFamily: "'Avenir Next', 'Avenir', 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'Noto Sans SC', system-ui, -apple-system, sans-serif"
    fontSize: "12px"
    fontWeight: 440
    lineHeight: 1.5
    letterSpacing: "0"
  label:
    fontFamily: "'Avenir Next', 'Avenir', 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'Noto Sans SC', system-ui, -apple-system, sans-serif"
    fontSize: "11px"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "0.07em"
  mono:
    fontFamily: "'JetBrains Mono', 'Fira Code', 'SFMono-Regular', Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace"
    fontSize: "12px"
    fontWeight: 440
    lineHeight: 1.5
    letterSpacing: "0"
rounded:
  control: "10px"
  input: "6px"
  theme-control: "12px"
  theme-thumb: "9px"
  card: "16px"
  pill: "999px"
spacing:
  compact: "8px"
  control-x: "13px"
  control-y: "8px"
  content-x: "30px"
  content-y: "32px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "#011222"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 {spacing.control-x}"
    height: "32px"
  button-primary-light:
    backgroundColor: "{colors.light-primary}"
    textColor: "#ffffff"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 {spacing.control-x}"
    height: "32px"
  button-ghost:
    backgroundColor: "rgba(255, 255, 255, 0.048)"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 {spacing.control-x}"
    height: "32px"
  button-ghost-light:
    backgroundColor: "rgba(12, 121, 207, 0.018)"
    textColor: "{colors.light-text}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 {spacing.control-x}"
    height: "32px"
  input:
    backgroundColor: "{colors.raised-panel}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.input}"
    padding: "0 14px"
    height: "36px"
  input-light:
    backgroundColor: "{colors.light-raised-panel}"
    textColor: "{colors.light-text}"
    typography: "{typography.body}"
    rounded: "{rounded.input}"
    padding: "0 14px"
    height: "36px"
  card:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
    rounded: "{rounded.card}"
    padding: "14px"
  card-light:
    backgroundColor: "{colors.light-panel}"
    textColor: "{colors.light-text}"
    rounded: "{rounded.card}"
    padding: "14px"
  navigation:
    backgroundColor: "{colors.raised-panel}"
    textColor: "{colors.muted-text}"
    typography: "{typography.navigation}"
    rounded: "{rounded.control}"
    padding: "0 {spacing.control-x}"
    height: "44px"
  navigation-light:
    backgroundColor: "{colors.light-raised-panel}"
    textColor: "{colors.light-muted-text}"
    typography: "{typography.navigation}"
    rounded: "{rounded.control}"
    padding: "0 {spacing.control-x}"
    height: "44px"
  theme-preference:
    backgroundColor: "rgba(255, 255, 255, 0.048)"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.theme-control}"
    padding: "0 {spacing.compact}"
    height: "42px"
  theme-preference-light:
    backgroundColor: "rgba(12, 121, 207, 0.018)"
    textColor: "{colors.light-text}"
    typography: "{typography.body}"
    rounded: "{rounded.theme-control}"
    padding: "0 {spacing.compact}"
    height: "42px"
---

# Design System: Dockrev

## Overview

**Creative North Star: "The Dual-Theme Ops Console"**

Dockrev is an operational interface for people working directly with Docker Compose services. It is a compact control surface: service state, version information, resource signals, actions, and outcomes must stay easy to scan while an operator is making a maintenance decision.

Dark is the default working theme, with a calm blue-black shell and restrained cyan signal color. A complete light theme mirrors the same information hierarchy for daylight use and personal preference; changing theme alters the palette, never the meaning of status or action. Both themes avoid a marketing or observability-poster feel in favor of durable admin-tool conventions.

**Key Characteristics:**
- A responsive app shell with compact navigation, top-bar context, and dense operational content.
- Theme-aware semantic tokens for action, information, success, warning, error, text, surface, and border.
- Small, deliberate controls and stable data layouts for repeated maintenance work.
- Status and risk are represented with text, icons, and color rather than color alone.
- Focus, disabled, reduced-motion, and narrow-screen behavior are part of the shared interaction language.

## Colors

The palette is semantic and theme-aware: deep blue-black surfaces and cyan signals in dark mode, clear blue-and-white surfaces in light mode, and the same operational state roles in each. The frontmatter records the dark defaults and paired `light-` overrides as literal values so DESIGN.md consumers do not need to resolve runtime CSS variables.

### Primary

- **Signal Primary**: the sole high-intent action, selected navigation, focus, and meaningful-link color.

### Secondary

- **Signal Secondary**: supporting information emphasis, chart differentiation, and restrained secondary signal.

### Tertiary

- **Operational Info**: active or informational state.
- **Operational Success**: healthy, completed, or allowed state.
- **Operational Warning**: stale, pending, recoverable, or attention-needed state.
- **Operational Error**: destructive, failed, blocked, or unrecoverable state.

### Neutral

- **Background, Panel, and Raised Panel**: the theme-specific shell and layered work surfaces.
- **Text and Muted Text**: readable operational copy and secondary metadata.
- **Border**: thin structure between dense controls, lists, and panels.

**The Semantic Signal Rule.** Primary and status colors communicate action or state. They are not decorative fill.

**The Theme Parity Rule.** Dark and light themes retain the same semantic roles, hierarchy, and interaction affordances.

## Typography

**Display Font:** Avenir Next with Avenir, Segoe UI, CJK system fonts, and system UI fallbacks.
**Body Font:** Avenir Next with the same system fallbacks.
**Label/Mono Font:** JetBrains Mono with platform monospace fallbacks for technical values.

**Character:** Typography is compact, legible, and native to an expert operational tool. Weight, grouping, and bounded line length establish hierarchy more often than oversized type.

### Hierarchy

- **Display**: page-level headings and the largest route context only.
- **Title**: service, modal, and content-panel titles.
- **Body**: dense descriptions, table cells, helper text, and operational explanations.
- **Label**: compact metadata and section labels; uppercase is reserved for short operational labels.
- **Mono**: identifiers, versions, paths, resource values, and other technical data.

**The Dense Sans Rule.** Keep the primary hierarchy in the shared sans stack; do not introduce decorative display or serif typography into product routes.

## Layout

The app shell uses a fixed desktop navigation column with a persistent top bar and a flexible content region. Detail routes may add a contextual side panel; the primary content still remains the scanning surface. Content uses a compact 30px horizontal and 32px vertical desktop rhythm, while cards, rows, and controls preserve stable dimensions as data changes.

At 1160px the shell contracts for medium widths. At 960px it becomes a single-column mobile shell with touch-sized controls and mobile navigation. At 640px dense groups and data layouts collapse further instead of overflowing. The body has a 320px minimum width, and narrow-screen state preserves the same routes, actions, and semantic status.

## Elevation & Depth

Dockrev uses tonal layering first and soft, localized shadow second. The shell, sidebars, and data rows stay structurally quiet; cards, popovers, drawers, and focused surfaces use elevation only when it explains a real stacking relationship. Theme-specific shadows are documented in the sidecar.

**The Layer Before Shadow Rule.** Establish hierarchy with a surface, border, and spacing before adding shadow.

**The One-Pixel Trace Rule.** Borders remain thin traces; operational status does not use thick colored stripes as decoration.

## Shapes

The form language is gently rounded and compact. Standard buttons and navigation controls use the control radius, inputs use the tighter input radius, panels use the card radius, and counts or compact status chips may use pill shapes. Borders are normally one pixel and low contrast; clipping and card geometry must support dense scanning instead of turning every section into a floating card.

## Components

### Buttons

- **Shape:** 32px controls with the control radius, compact horizontal padding, and a stable label or icon slot.
- **Primary:** a theme-aware primary signal with readable semantic foreground and a restrained depth cue for high-intent operations.
- **Ghost:** a quiet surface and thin border for secondary commands.
- **Danger:** the error role is reserved for destructive or rollback-risk actions.
- **Hover / Focus:** hover moves a control by one pixel at most; keyboard focus is a visible offset ring.

### Chips

- **Style:** compact neutral surfaces with a full pill or soft-radius shape and short labels.
- **State:** selected chips and navigation states may use a primary tint; inactive chips remain quiet.

### Cards / Containers

- **Corner Style:** cards use the card radius while denser rows and nested controls stay tighter.
- **Background:** theme-aware panel layers with restrained tonal variation.
- **Shadow Strategy:** default surfaces stay quiet; floating or focused surfaces use the documented shadow vocabulary.
- **Border:** one-pixel semantic border.
- **Internal Padding:** compact panels start from the shared compact rhythm and expand only where the content needs it.

### Inputs / Fields

- **Style:** 36px theme-aware fields with the tighter input radius and a thin border.
- **Focus:** visible focus ring with primary signal; no focus state relies on color alone.
- **Error / Disabled:** errors use the semantic error role; disabled controls remain visibly unavailable and do not offer pointer affordance.

### Navigation

- **Desktop:** vertical navigation and a top bar keep route identity, context, and actions close to the work.
- **Responsive:** the desktop shell contracts at medium widths and becomes a mobile navigation pattern below the mobile breakpoint.
- **Active State:** an active item uses a primary-tinted surface, stronger label weight, and the primary icon color.

### Theme Preference

- **Style:** a compact icon control on constrained navigation and a segmented control where the full setting is visible.
- **Behavior:** switching themes preserves the semantic meaning of the product and honors reduced-motion preferences.

**The Theme Pair Rule.** Use the matching `light-` primitive when a component is rendered in the light theme. Dark tokens are the default primitives; the runtime's semantic aliases select the equivalent light tokens without changing action or status meaning.

## Do's and Don'ts

### Do:
- **Do** keep pages compact, dense, and scannable for repeated maintenance work.
- **Do** use the semantic state roles consistently in both themes.
- **Do** keep real, stale, disabled, unknown, and failed states explicit.
- **Do** maintain visible keyboard focus and usable 320px-to-desktop layouts.
- **Do** place high-intent actions beside the service or job evidence that supports the decision.

### Don't:
- **Don't** turn authenticated operational routes into marketing landing pages.
- **Don't** use color or decorative status in place of truthful operational data.
- **Don't** introduce a generic dashboard card grid where structured rows or grouped service data are clearer.
- **Don't** add neon, purple-gradient, glass-first, or oversized decorative styling as product identity.
- **Don't** let dark and light themes diverge in status meaning, interaction affordance, or information hierarchy.
