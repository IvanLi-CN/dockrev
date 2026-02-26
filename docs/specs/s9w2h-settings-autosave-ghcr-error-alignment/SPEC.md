# Dockrev：Settings 自动保存串行化 + GHCR 错误归因 + PAT 预校验（#s9w2h）

## 状态

- Status: 已完成
- Created: 2026-02-26
- Last: 2026-02-26

## 背景 / 问题陈述

- 设置页此前依赖手动保存，且不同字段可能并行保存，用户在快速编辑时容易触发“状态错位”与误导性错误提示。
- `解析并添加` 与 GHCR 设置保存存在时序耦合，出现“未保存就解析”与“错误原因不真实”的体验问题。
- 用户明确要求：自动保存必须串行、错误提示必须与真实原因对齐、PAT 相关必须在自动保存前先校验。

## 目标 / 非目标

### Goals

- Settings 页改为自动保存，并保证全局串行（任意时刻最多一个保存请求在飞）。
- `解析并添加` 强制先 `flush(['ghcr'])`，保存失败即阻断解析。
- GHCR 解析错误按可判别 reason 映射精准提示，禁止语义错位。
- 运行时不可持久化的 Forward Header 配置改为只读展示，避免伪可编辑交互。
- PAT 在进入自动保存前进行前端预校验（格式与必填条件），无效时直接阻断并给出字段错误。

### Non-goals

- 不新增 REST 路径或破坏现有请求体兼容性。
- 不引入新的后端持久化模型（仅使用现有 settings 结构）。
- 不做 UI 视觉主题重构。

## 范围（Scope）

### In scope

- `web/src/pages/SettingsPage.tsx`
- `web/src/App.css`
- `web/src/api.ts`（错误 reason 消费兼容）
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/tests.rs`

### Out of scope

- GHCR 以外 registry webhook 交互重构。
- 新的权限系统或 GitHub OAuth 接入。

## 需求（Requirements）

### MUST

- 自动保存全局串行：单队列 + 单 worker，禁止并行保存。
- 同 scope 变更合并提交，重复 payload 去重，连续输入仅提交最终值。
- `flush(scopes?)` 可阻塞等待保存完成，供 `解析并添加` 严格门控。
- `解析并添加` 执行顺序固定为：本地校验 -> `flush(['ghcr'])` -> resolve/add。
- 错误语义映射必须可区分：
  - `ghcr_pat_missing`
  - `ghcr_pat_unsaved_or_save_failed`
  - `ghcr_pat_invalid_or_scope_insufficient`
  - `github_upstream_timeout`
  - `github_upstream_unavailable`
- PAT 预校验必须在自动保存请求发出前执行：
  - 当 `enabled=true` 且既无已保存 PAT 也无本次显式 PAT 时，阻断保存并提示 `请先填写 GitHub PAT`。
  - 当 PAT 显式输入但格式非法（非 `ghp_`/`github_pat_` 等 GitHub token 前缀或含空白字符）时，阻断保存并提示格式错误。
  - 预校验失败不得发出 GHCR `PUT` 请求。

### SHOULD

- 自动保存状态提示不占主布局空间（浮层/非占位）。
- 字段级错误优先贴近 PAT 输入区域展示，且不得触发布局抖动（reflow/jank）。

## 验收标准（Acceptance Criteria）

- Given 连续修改 backup/notifications/ghcr，When 观察网络请求，Then 保存请求严格串行且无重叠。
- Given 同字段快速输入多次，When debounce 周期结束，Then 仅提交最终值。
- Given 点击 `解析并添加`，When GHCR 存在待保存变更，Then 先完成 `flush(['ghcr'])` 再发 resolve。
- Given GHCR 保存失败或预校验失败，When 点击 `解析并添加`，Then resolve 不发起，UI 显示真实原因。
- Given PAT 输入 `abc`（非法格式），When 自动保存触发，Then 不发送 GHCR `PUT`，并显示 `PAT 格式不合法...`。

## 里程碑（Milestones / checklist）

- [x] M1: 串行自动保存协调器（队列/去重/flush/状态）落地。
- [x] M2: `解析并添加` 改为 `flush ghcr -> resolve` 门控链路。
- [x] M3: 后端 GHCR resolve 错误 reason 语义化。
- [x] M4: 运行时 auth 字段改为只读展示，移除误导性交互。
- [x] M5: PAT 自动保存前预校验（格式 + 必填）与 Storybook 可视化验收。

## 风险 / 假设

- 风险：PAT 前缀规则过严可能误伤未来 token 形态；需保留后端兜底校验。
- 假设：`******` 继续作为“保留已存 PAT”的保留值语义。

## 变更记录（Change log）

- 2026-02-26: 新建规格，收敛自动保存串行、解析门控与 GHCR 错误归因口径。
- 2026-02-26: 根据验收反馈补充 M5（PAT 自动保存前预校验）为硬性需求，禁止“先发请求再报错”。
- 2026-02-26: 完成 PAT 保存前预校验（格式 + 必填）与 Storybook 可视化验证，规格收口为已完成。
- 2026-02-26: 根据交互反馈移除 PAT 行内错误文本，改为输入框高亮 + 非占位浮层提示，避免界面抖动。
