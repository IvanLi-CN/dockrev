# Dockrev：Supervisor 暗色主题修复 + 同源主题偏好共享（#bvxtm）

## 状态

- Status: 已完成
- Created: 2026-03-09
- Last: 2026-03-09
- Notes: fast-track

## 背景 / 问题陈述

- 当前 `/supervisor/` 为独立的 inline HTML/CSS/JS 页面，样式固定偏向浅色，在 dark 用户环境下可读性与观感明显退化。
- Dockrev 主程序已经约定使用 `localStorage["dockrev:theme"]` 持久化主题偏好，但 Supervisor 尚未复用该 contract，导致同源下主程序与自我升级页主题割裂。
- 若不补齐主题支持，运维在夜间或 dark 设备环境操作自我升级时会遇到首屏白闪、对比度不足与跨页面主题不一致的问题。

## 目标 / 非目标

### Goals

- 为 `/supervisor/` 补齐 light/dark 双主题，并在首屏渲染前完成主题 bootstrap，避免 dark 偏好下的白闪。
- 复用主程序现有主题偏好 contract：优先读取 `localStorage["dockrev:theme"]`，无有效值时回退 `prefers-color-scheme`。
- 在同源场景下，主程序页面修改主题偏好后，已打开的 Supervisor 页面也能同步切换而不重置当前日志/状态视图。
- 保持 Supervisor 现有自升级 API、轮询、按钮状态机与 rollback 确认交互不变。

### Non-goals

- 不实现跨域主题透传或 URL theme hint。
- 不新增 Supervisor 独立主题切换控件。
- 不把 Supervisor 改写成 React 页面。
- 不调整 `/supervisor/health`、`/supervisor/version`、`/supervisor/self-upgrade` 的 schema。

## 范围（Scope）

### In scope

- `crates/dockrev-supervisor/src/app/ui.rs`：主题 bootstrap、主题同步脚本与 light/dark token 化样式。
- `crates/dockrev-supervisor/src/app/tests.rs`：render_ui 字符串锚点回归测试。
- `docs/specs/README.md`：新增索引并在实现完成后同步状态。

### Out of scope

- `web/src/**` 主程序主题 contract 的改名或存储策略调整。
- 自升级执行逻辑、日志分组算法、鉴权/路由策略。
- 需要跨站点共享偏好的部署拓扑。

## 需求（Requirements）

### MUST

- `/supervisor/` 必须支持 `html[data-theme='light'|'dark']` 两套主题表现。
- 页面必须优先读取 `localStorage["dockrev:theme"]`；仅在缺失、非法或读取失败时回退系统主题。
- 同源其他页面改写主题偏好后，Supervisor 必须通过 `storage` 事件同步主题。
- 当没有已保存偏好时，系统主题变化必须驱动 Supervisor 更新主题。
- 既有 dry-run/apply/rollback/refresh 行为与轮询频率必须保持兼容。

### SHOULD

- dark/light 下 body、card、button、input、pre、operation tabs、rollback popconfirm、status 文本与链接都保持可读对比度。
- 主题同步不应影响当前 operation 选中态或日志内容。

### COULD

- 保持与主程序近似的蓝/青视觉方向，减少产品割裂感。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 用户打开 `/supervisor/` 时，head 内联脚本在首屏样式应用前决定主题，并把结果写入 `document.documentElement.dataset.theme` 与 `color-scheme`。
- 若存在有效的 `localStorage["dockrev:theme"]`，页面固定跟随该偏好；若不存在，则跟随 `prefers-color-scheme`。
- 页面加载完成后继续轮询 `self-upgrade` 状态；主题同步只更新 DOM 主题属性，不修改日志、active operation、按钮运行态或 polling。
- 其它同源 Dockrev 页面修改 `dockrev:theme` 后，Supervisor 通过 `storage` 事件立即刷新主题。

### Edge cases / errors

- `localStorage` 读取异常、值非法或浏览器禁用存储时，页面回退到系统主题，不阻断已有脚本运行。
- 浏览器仅支持旧版 `MediaQueryList.addListener` 时，仍需兼容系统主题变化监听。
- 当 `storage` 事件收到无关 key 时，Supervisor 不做额外更新。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /supervisor/` | HTML UI | external | Modify | None | supervisor | operator | 新增主题 bootstrap、同步脚本与双主题样式 |
| `localStorage["dockrev:theme"]` | browser storage contract | internal | Reuse | None | web + supervisor | Dockrev main UI / supervisor UI | 不改 key，不改值域，仅复用 `light` / `dark` |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given supervisor origin 下 `localStorage["dockrev:theme"]="dark"`，When 打开 `/supervisor/`，Then 页面首屏直接进入 dark 主题，且 body/card/button/input/logs/popconfirm/tabs/status 文本均可读。
- Given supervisor origin 下 `localStorage["dockrev:theme"]="light"`，When 打开 `/supervisor/`，Then 页面保持 light 主题，且现有 dry-run/apply/rollback/refresh 交互不退化。
- Given 主题 key 缺失、非法或读取失败，When 打开 `/supervisor/`，Then 页面回退到 `prefers-color-scheme`，且自升级轮询、日志刷新、按钮状态逻辑继续正常工作。
- Given Supervisor 页面已打开，When 同源其他 Dockrev 页面改写 `dockrev:theme`，Then Supervisor 主题同步更新且不清空当前日志、不重置当前 operation 选中态。
- Given 访问 `/supervisor/health`、`/supervisor/version`、`/supervisor/self-upgrade`，When 执行本次改动后回归，Then 返回 schema 与已有行为保持兼容。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `cargo test -p dockrev-supervisor`
- Browser smoke: 本地打开 `/supervisor/`，验证 dark/light 首屏、storage 同步与系统主题回退。

### Quality checks

- 保持现有 Rust 格式与字符串输出测试稳定。

## 文档更新（Docs to Update）

- `docs/specs/README.md`: 新增索引并同步最终状态/备注。

## 计划资产（Plan assets）

- None

## Visual Evidence (PR)

## 资产晋升（Asset promotion）

- None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 为 Supervisor 页面增加 head 级主题 bootstrap，并复用 `dockrev:theme` contract。
- [x] M2: 将 Supervisor inline CSS 重构为 light/dark 双主题 token，覆盖关键操作区与日志区。
- [x] M3: 增加 render_ui 单测，锁住主题 bootstrap、storage 同步与系统主题回退锚点。
- [x] M4: 通过 `cargo test -p dockrev-supervisor` 与浏览器 smoke 验证。

## 方案概述（Approach, high-level）

- 保持 Supervisor 为无依赖静态 HTML 页面，只在 head/body 脚本中加入最小主题控制器与监听逻辑。
- 使用 CSS 变量统一 light/dark 颜色，而非为现有 DOM 大幅增删 class，降低回归面。
- 主题同步只修改根节点 `data-theme`/`color-scheme`，避免干扰自升级轮询与操作状态机。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：inline HTML 字符串变长，render_ui 锚点测试需同步补强，避免后续回归误删主题逻辑。
- 风险：旧浏览器对 `matchMedia` 监听 API 支持不一致，需要 addEventListener/addListener 双兼容。
- 假设：Supervisor 与主程序同源部署时可共享 `localStorage["dockrev:theme"]`；跨域部署不在本次范围内。

## 变更记录（Change log）

- 2026-03-09: 创建规格，冻结“Supervisor dark theme + 同源主题共享”范围、验收标准与实现里程碑。
- 2026-03-09: 完成 Supervisor theme bootstrap、light/dark token、storage/system 同步监听、render_ui 单测与本地浏览器 smoke 验证。

## 参考（References）

- `web/src/theme.ts`
- `crates/dockrev-supervisor/src/app/ui.rs`
