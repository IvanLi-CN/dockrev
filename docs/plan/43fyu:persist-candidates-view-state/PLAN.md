# Dockrev Web: 记住更新候选页视图状态（tag in URL + stack 展开/折叠）（#43fyu）

## 状态

- Status: 待实现
- Created: 2026-02-06
- Last: 2026-02-06

## 背景 / 问题陈述

- 更新候选页存在“标签(tab)筛选 + 按 stack 折叠的服务列表”交互。
- 现状：刷新/重新访问后，标签与展开/折叠状态会丢失，影响连续操作与分享链接。

## 目标 / 非目标

### Goals

- 选中的“标签(tab)”写入 URL，并在重新访问/刷新/前进后退时正确恢复。
- 记住服务列表中各 stack 的展开/折叠状态，并且按标签分别保存与恢复（不同标签互不影响）。
- URL 中的标签参数非法/缺失时有合理降级（回退默认标签）。

### Non-goals

- 不引入后端改动/存储；不做跨设备同步（仍以浏览器本地存储为准）。
- 不改变现有标签的业务含义与数据来源。

## 范围（Scope）

### In scope

- Web 路由：
  - 在候选页路由上加入/读取 `tab`（或等价参数）来表达当前标签。
  - 标签切换时更新 URL（支持浏览器 back/forward）。
- UI state persistence:
  - 将 stack 的展开/折叠状态持久化到 `localStorage`（或现有的持久化方案）。
  - key 设计包含当前标签，避免不同标签互相污染。
  - 数据加载后应用持久化状态（优先于默认展开策略）。
- 适配：当候选数据变化导致 stack 不再存在时，持久化状态应自动忽略（避免报错）。

### Out of scope

- 迁移/重命名现有路由结构（除非必须）。
- 为每个用户账号做服务端持久化。

## 验收标准（Acceptance Criteria）

- Given 我在更新候选页选择标签 `需确认`，
  When 刷新页面或复制链接重新打开，
  Then 页面仍停留在 `需确认` 标签，并且 URL 明确包含该标签参数。

- Given 我在标签 A 下把 stack `ai` 折叠，
  When 刷新页面或重新访问，
  Then 标签 A 下 `ai` 仍保持折叠；且切换到标签 B 时不会继承标签 A 的折叠状态。

- Given URL 中的标签参数未知/非法，
  When 打开页面，
  Then 页面回退到默认标签，且不报错。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- 增加/调整至少一个前端测试，覆盖：
  - URL 参数 -> 初始标签选择
  - 按标签隔离的折叠状态读写（localStorage key + reducer/hook）

### Quality checks

- `web` 的 typecheck/build 至少通过一项（以仓库已有脚本为准）。

## 实现里程碑（Milestones）

- [ ] M1: 标签写入 URL + 初始恢复 + back/forward 行为
- [ ] M2: stack 展开/折叠状态持久化（按标签隔离）
- [ ] M3: 补齐测试与最小验证

## 风险 / 备注（Risks / Notes）

- 需要选定一个“稳定的 stack 标识”（例如 stack slug/name）。若存在重名/空值，需要明确 fallback。
- 注意避免 localStorage 写入过于频繁（必要时做去抖或仅在交互时写）。

## 变更记录（Change log）

- 2026-02-06: 创建计划并冻结验收标准；状态设为 `待实现`。

