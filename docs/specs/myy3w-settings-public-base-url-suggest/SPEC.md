# Dockrev：Settings 空 Public Base URL 当前地址建议气泡（#myy3w）

## 状态

- Status: 已完成
- Created: 2026-03-07
- Last: 2026-03-07

## 背景 / 问题陈述

- Settings 页的实例 `Public Base URL` 允许留空，但首次部署或从空白配置进入时，用户往往不知道该填什么。
- 当前页面已经具备可直接复用的站点根地址，缺少一个就地建议入口，导致用户需要手动复制粘贴。
- 用户要求：当字段为空时自动出现一个非占位气泡，直接提示使用当前网页地址，并允许一键填入或永久拒绝该建议。

## 目标 / 非目标

### Goals

- 当 `instance.publicBaseUrl` 为空（含全空白字符）且未被本浏览器拒绝时，在输入框下方显示当前站点根地址建议气泡。
- 候选值固定使用当前页面 `origin + '/'`，例如 `https://dockrev.ivanli.cc/`，不带 `/settings`、query 或 hash。
- 气泡中的候选地址使用 `Mono` 内联 code 样式展示，按钮文案固定为 `自动填入` 与 `不`。
- 点击 `自动填入` 复用现有 `updateInstance('instance.publicBaseUrl', ...)` 与 autosave 链路，不新增后端接口。
- 点击 `不` 后用 `localStorage['dockrev:settings:instancePublicBaseUrl:suggestCurrentOriginDismissed'] = '1'` 记住偏好，下次同浏览器不再显示。

### Non-goals

- 不修改后端 settings schema、数据库字段或通知链接生成逻辑。
- 不把拒绝偏好同步到服务端或其它设备。
- 不引入全局 toast、弹窗或额外设置项。

## 范围（Scope）

### In scope

- `web/src/pages/SettingsPage.tsx`
- `web/src/App.css`
- `web/src/stories/pages/SettingsPage.stories.tsx`
- `docs/specs/README.md`

### Out of scope

- `crates/dockrev-api/**`
- `web/src/api.ts`
- 其它设置卡片或通知卡片交互

## 需求（Requirements）

### MUST

- 建议气泡只在以下条件同时成立时显示：
  - `settings.instance.publicBaseUrl` 为 `null`、空字符串或仅空白字符；
  - 本地拒绝偏好不存在；
  - 当前运行环境可解析出有效的 `window.location.origin`。
- 建议文案固定为“是否使用当前地址 `<Mono>{originWithTrailingSlash}</Mono>`？”。
- `自动填入` 必须把输入框值设置为候选地址，并走现有 autosave 队列。
- `不` 必须立即隐藏当前页气泡，并尽力写入 localStorage；若写入失败，当前页隐藏仍然成立。
- 输入框出现任意非空白值时，建议气泡必须消失；若用户之后再次清空且未拒绝，允许重新出现。

### SHOULD

- 气泡样式应与现有 Settings 视觉风格一致，作为输入框下的 info-style inline bubble，而不是遮挡式浮层。
- Storybook 应有可复现的两个场景：建议可见并自动填入、已拒绝后刷新不再显示。

## 验收标准（Acceptance Criteria）

- Given `publicBaseUrl` 为空且 localStorage 未拒绝，When 打开 Settings，Then Public Base URL 输入框下方出现建议气泡，且显示当前站点根地址。
- Given 当前页面路径为 `/settings`，When 生成候选地址，Then 展示值为 `origin + '/'`，而不是包含 `/settings` 的完整 URL。
- Given 用户点击 `自动填入`，When 交互完成，Then 输入框被填入候选地址，建议气泡立即消失，后续保存继续复用现有 autosave。
- Given 用户点击 `不`，When 刷新页面且字段仍为空，Then 建议气泡不再显示。
- Given 输入框已有任意非空白值，When 页面渲染，Then 建议气泡不存在。

## 里程碑（Milestones / checklist）

- [x] M1: Settings 页新增当前站点根地址候选值与 localStorage 拒绝偏好 helper。
- [x] M2: Public Base URL 输入区新增 inline suggestion bubble 与 `自动填入` / `不` 交互。
- [x] M3: Storybook 新增可见态与已拒绝态场景。
- [x] M4: `lint` / `build` / `build-storybook` / `test-storybook` 回归通过。

## 风险 / 假设

- 假设：`window.location.origin` 在生产部署与 Storybook 环境下都可用，并且用作实例对外访问根地址是合理默认值。
- 风险：若部署实际需要子路径而不是站点根地址，本建议只提供 origin 级默认值，用户仍可手动改写。

## 变更记录（Change log）

- 2026-03-07: 新建规格并冻结“空 Public Base URL 当前地址建议气泡”的交互口径。
- 2026-03-07: 完成 Settings inline suggestion bubble、localStorage 拒绝偏好与 Storybook 场景；`bun run --cwd web lint`、`build`、`build-storybook`、`test-storybook` 通过。
