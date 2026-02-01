# Dockrev: /supervisor 日志区域换行修复（#9wyaj）

## 状态

- Status: 已完成
- Created: 2026-02-01
- Last: 2026-02-01

## 背景 / 问题陈述

- `/supervisor` 页面用于 Dockrev 自我升级（Supervisor）。
- 现状：日志区域把换行符显示为字面量 `\n`，导致日志挤在一行，难以阅读与排查。

## 目标 / 非目标

### Goals

- 日志区域按行展示：支持真实换行（LF/CRLF），也能正确处理字面量 `\n`（双重转义）输入。
- 保持日志内容可复制（copy/paste 后仍为多行文本）。

### Non-goals

- 不改变日志内容来源、日志级别、过滤等业务逻辑。
- 不引入复杂的日志解析（例如结构化字段高亮）。

## 范围（Scope）

### In scope

- `/supervisor` 页面日志展示：
  - 渲染时将字面量 `\\n`（以及 `\\r\\n`）解码为实际换行；
  - 使用合适的 CSS 保留换行并允许长行自动换行（例如 `white-space: pre-wrap`）。
- 最小测试覆盖：对日志文本格式化/渲染逻辑提供回归测试。

### Out of scope

- Supervisor API 返回格式调整（除非确认问题在后端转义）。

## 需求（Requirements）

### MUST

- 当日志字符串包含实际换行字符 `\n` / `\r\n` 时，UI 必须按多行展示。
- 当日志字符串包含字面量 `\\n`（或 `\\r\\n`）时，UI 必须解码后按多行展示。
- 日志区域不应因换行导致布局溢出：长行可换行或可滚动（按现有 UI 风格）。

### SHOULD

- 保持等宽字体与可滚动容器（如现有）。
- 不破坏现有状态信息（status / opId / step 等）展示。

## 验收标准（Acceptance Criteria）

- Given 打开 `https://dockrev.ivanli.cc/supervisor`
  When 日志内容包含字面量 `\\n`
  Then 页面按行显示，不再出现字面量 `\n`
- Given 同上
  When 日志内容包含真实换行
  Then 页面按行显示且不会折叠成一行
- Given 同上
  When 复制日志文本
  Then 粘贴到文本编辑器后仍保持多行

## 里程碑（Milestones）

- [x] 修复日志渲染：解码 `\\n` 并保留换行
- [x] 补充最小回归测试

## 风险与开放问题（Risks & Open Questions）

- 如果 `\\n` 来自后端双重转义，可能需要后端一并修正；优先以不破坏兼容为原则在前端兜底处理。
