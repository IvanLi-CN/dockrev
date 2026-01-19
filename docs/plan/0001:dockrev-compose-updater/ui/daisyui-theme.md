# Dockrev DaisyUI Theme (Draft)

本目录下的 UI 设计图已按 DaisyUI 的 **Theme tokens**（`base-* / primary / secondary / accent / neutral / info / success / warning / error`）去对齐，目标是让这套配色能直接落成 DaisyUI 的自定义主题，并且保持：

- `--depth: 0`（无阴影/3D）
- `--noise: 0`（无颗粒噪点）

## Theme values

### `dockrev-light`

- `--color-base-100`: `#ffffff`
- `--color-base-200`: `#f7f8fb`
- `--color-base-300`: `#edf2f7`
- `--color-base-content`: `#0f172a`

- `--color-primary`: `#0ea5e9`
- `--color-primary-content`: `#ffffff`
- `--color-secondary`: `#6366f1`
- `--color-secondary-content`: `#ffffff`
- `--color-accent`: `#22c55e`
- `--color-accent-content`: `#052e16`
- `--color-neutral`: `#0f172a`
- `--color-neutral-content`: `#ffffff`

- `--color-info`: `#38bdf8`
- `--color-info-content`: `#001018`
- `--color-success`: `#16a34a`
- `--color-success-content`: `#052e16`
- `--color-warning`: `#f59e0b`
- `--color-warning-content`: `#1a1200`
- `--color-error`: `#e11d48`
- `--color-error-content`: `#ffffff`

#### Dockrev extra tokens

为降低“卡片内子区域背景色差异过大”的观感（扁平化、弱分隔），UI 里会额外使用一层 **subtle surface**（用于表单行、分组 header 等）。该 token 不是 DaisyUI 必需字段，但可以和主题一起定义，便于后续统一调色。

- `--dockrev-surface`: `rgba(15,23,42,.03)`

### `dockrev-dark`

- `--color-base-100`: `#0f172a`
- `--color-base-200`: `#0b1020`
- `--color-base-300`: `#070b15`
- `--color-base-content`: `#e5e7eb`

- `--color-primary`: `#38bdf8`
- `--color-primary-content`: `#001018`
- `--color-secondary`: `#a78bfa`
- `--color-secondary-content`: `#0b1020`
- `--color-accent`: `#22c55e`
- `--color-accent-content`: `#041008`
- `--color-neutral`: `#111827`
- `--color-neutral-content`: `#e5e7eb`

- `--color-info`: `#38bdf8`
- `--color-info-content`: `#001018`
- `--color-success`: `#22c55e`
- `--color-success-content`: `#041008`
- `--color-warning`: `#f59e0b`
- `--color-warning-content`: `#1a1200`
- `--color-error`: `#fb7185`
- `--color-error-content`: `#1a0006`

#### Dockrev extra tokens

- `--dockrev-surface`: `rgba(255,255,255,.035)`

## DaisyUI v5 plugin format (CSS)

> 下面片段用于 `@plugin "daisyui/theme" { ... }`（DaisyUI v5）方式定义主题。

```css
@plugin "daisyui/theme" {
  name: "dockrev-light";
  default: true;
  prefersdark: false;
  color-scheme: light;

  --color-base-100: #ffffff;
  --color-base-200: #f7f8fb;
  --color-base-300: #edf2f7;
  --color-base-content: #0f172a;

  --color-primary: #0ea5e9;
  --color-primary-content: #ffffff;
  --color-secondary: #6366f1;
  --color-secondary-content: #ffffff;
  --color-accent: #22c55e;
  --color-accent-content: #052e16;
  --color-neutral: #0f172a;
  --color-neutral-content: #ffffff;

  --color-info: #38bdf8;
  --color-info-content: #001018;
  --color-success: #16a34a;
  --color-success-content: #052e16;
  --color-warning: #f59e0b;
  --color-warning-content: #1a1200;
  --color-error: #e11d48;
  --color-error-content: #ffffff;

  /* Dockrev UI: subtle row/group surface */
  --dockrev-surface: rgba(15,23,42,.03);

  --radius-selector: 1rem;
  --radius-field: 0.75rem;
  --radius-box: 1rem;
  --size-selector: 0.25rem;
  --size-field: 0.25rem;
  --border: 1px;
  --depth: 0;
  --noise: 0;
}

@plugin "daisyui/theme" {
  name: "dockrev-dark";
  default: false;
  prefersdark: true;
  color-scheme: dark;

  --color-base-100: #0f172a;
  --color-base-200: #0b1020;
  --color-base-300: #070b15;
  --color-base-content: #e5e7eb;

  --color-primary: #38bdf8;
  --color-primary-content: #001018;
  --color-secondary: #a78bfa;
  --color-secondary-content: #0b1020;
  --color-accent: #22c55e;
  --color-accent-content: #041008;
  --color-neutral: #111827;
  --color-neutral-content: #e5e7eb;

  --color-info: #38bdf8;
  --color-info-content: #001018;
  --color-success: #22c55e;
  --color-success-content: #041008;
  --color-warning: #f59e0b;
  --color-warning-content: #1a1200;
  --color-error: #fb7185;
  --color-error-content: #1a0006;

  /* Dockrev UI: subtle row/group surface */
  --dockrev-surface: rgba(255,255,255,.035);

  --radius-selector: 1rem;
  --radius-field: 0.75rem;
  --radius-box: 1rem;
  --size-selector: 0.25rem;
  --size-field: 0.25rem;
  --border: 1px;
  --depth: 0;
  --noise: 0;
}
```

## CSS custom properties format (`data-theme=...`)

> DaisyUI 也支持用 `[data-theme="..."]` 定义主题。不同版本/构建链对变量命名（`--primary` vs `--color-primary`）可能略有差异，建议以项目实际 DaisyUI 版本为准。

```css
[data-theme="dockrev-light"] {
  --primary: #0ea5e9;
  --primary-content: #ffffff;
  --secondary: #6366f1;
  --secondary-content: #ffffff;
  --accent: #22c55e;
  --accent-content: #052e16;
  --neutral: #0f172a;
  --neutral-content: #ffffff;

  --base-100: #ffffff;
  --base-200: #f7f8fb;
  --base-300: #edf2f7;
  --base-content: #0f172a;

  --info: #38bdf8;
  --info-content: #001018;
  --success: #16a34a;
  --success-content: #052e16;
  --warning: #f59e0b;
  --warning-content: #1a1200;
  --error: #e11d48;
  --error-content: #ffffff;

  --radius-selector: 1rem;
  --radius-field: 0.75rem;
  --radius-box: 1rem;
  --size-selector: 0.25rem;
  --size-field: 0.25rem;
  --border: 1px;
  --depth: 0;
  --noise: 0;
}

[data-theme="dockrev-dark"] {
  --primary: #38bdf8;
  --primary-content: #001018;
  --secondary: #a78bfa;
  --secondary-content: #0b1020;
  --accent: #22c55e;
  --accent-content: #041008;
  --neutral: #111827;
  --neutral-content: #e5e7eb;

  --base-100: #0f172a;
  --base-200: #0b1020;
  --base-300: #070b15;
  --base-content: #e5e7eb;

  --info: #38bdf8;
  --info-content: #001018;
  --success: #22c55e;
  --success-content: #041008;
  --warning: #f59e0b;
  --warning-content: #1a1200;
  --error: #fb7185;
  --error-content: #1a0006;

  --radius-selector: 1rem;
  --radius-field: 0.75rem;
  --radius-box: 1rem;
  --size-selector: 0.25rem;
  --size-field: 0.25rem;
  --border: 1px;
  --depth: 0;
  --noise: 0;
}
```

## Tailwind config format (JS)

> 下面片段用于 `tailwind.config.{js,ts}` 中的 `daisyui.themes`（对象键名与颜色键名按 DaisyUI 约定）。

```js
// tailwind.config.js
export default {
  // ...
  daisyui: {
    themes: [
      {
        "dockrev-light": {
          primary: "#0ea5e9",
          "primary-content": "#ffffff",
          secondary: "#6366f1",
          "secondary-content": "#ffffff",
          accent: "#22c55e",
          "accent-content": "#052e16",
          neutral: "#0f172a",
          "neutral-content": "#ffffff",

          "base-100": "#ffffff",
          "base-200": "#f7f8fb",
          "base-300": "#edf2f7",
          "base-content": "#0f172a",

          info: "#38bdf8",
          "info-content": "#001018",
          success: "#16a34a",
          "success-content": "#052e16",
          warning: "#f59e0b",
          "warning-content": "#1a1200",
          error: "#e11d48",
          "error-content": "#ffffff",
        },
      },
      {
        "dockrev-dark": {
          primary: "#38bdf8",
          "primary-content": "#001018",
          secondary: "#a78bfa",
          "secondary-content": "#0b1020",
          accent: "#22c55e",
          "accent-content": "#041008",
          neutral: "#111827",
          "neutral-content": "#e5e7eb",

          "base-100": "#0f172a",
          "base-200": "#0b1020",
          "base-300": "#070b15",
          "base-content": "#e5e7eb",

          info: "#38bdf8",
          "info-content": "#001018",
          success: "#22c55e",
          "success-content": "#041008",
          warning: "#f59e0b",
          "warning-content": "#1a1200",
          error: "#fb7185",
          "error-content": "#1a0006",
        },
      },
    ],
  },
}
```
