# Dockrev：设置页新增 Cron 定时任务（定期检查更新 + Webhook 巡查）+ /queue 模块名修正（#keynr）

## 状态

- Status: 已完成
- Created: 2026-03-05
- Last: 2026-03-05

## 背景 / 问题陈述

- 当前 Dockrev 已有若干“进程内 interval/sleep”周期任务（如 discovery/runtime scan/GHCR audit），但缺少面向操作者的“可配置计划任务（cron）”能力。
- 运维侧希望能在 Settings 中显式控制：
  - “定期检查更新”（只做 check 刷新候选版本，不自动 apply update）
  - “webhook 巡查”（GHCR audit_all：检测 drift，不自动修复）
- 同时，`/queue` 页面目前部分 UI 文案仍叫“更新队列”，与页面实际内容（展示各类任务）不一致。

## 目标 / 非目标

### Goals

- Settings（系统设置）新增两组定时任务配置：
  - 定期检查更新：任务开关 + cron
  - webhook 巡查（GHCR audit_all）：任务开关 + cron
- cron 表达式按**服务端本地时区**解释（`chrono::Local`），并补齐容器运行时的 tz 支持。
- 任务到点执行时：
  - “定期检查更新”创建 `check` job（`createdBy=schedule`、`reason=schedule`、`scope=all`），并在任务队列中可见
  - “webhook 巡查”入队 `github_packages_webhook` 的 `audit_all` job（同样 `createdBy/reason=schedule`），在 `/queue` 与 GHCR 队列页可见
- `/queue` 模块名统一为“任务队列”（侧栏导航 + 顶栏 hint）。

### Non-goals

- 不支持按 stack/service 细粒度配置 cron（本期仅全局）。
- 不实现“定期自动更新（apply update）”；定期任务仅触发 check 与 GHCR 巡查。
- 不在 Settings 中展示“下一次触发时间/上一次触发时间”（后续可加）。

## 范围（Scope）

### In scope

- Backend:
  - DB: `settings` 表新增 schedule 相关列（兼容 migrate）。
  - API: `GET/PUT /api/settings` 扩展 schedules 字段，cron 校验与错误 reason。
  - Runtime: 新增 cron scheduler（进程内），触发 check job 与 GHCR audit job；并替换原 GHCR interval scheduler，避免双跑。
  - Container: runtime-base 增加 tzdata；deploy compose 增加 `TZ=Asia/Shanghai`（默认）。
- Frontend:
  - `web/src/pages/SettingsPage.tsx` 新增“定时任务”卡片（开关 + cron 输入、自动保存、错误提示）。
  - `web/src/api.ts` SettingsResponse/PutSettingsInput 扩展。
  - Storybook mock 同步。
  - `/queue` 文案统一“任务队列”。

### Out of scope

- 新增额外 endpoint（如“预览下一次触发”）。
- 变更任务队列数据模型（沿用现有 jobs 表与 SSE）。

## 需求（Requirements）

### MUST

- Settings API:
  - `GET /api/settings` 返回 `schedules.updateCheck` 与 `schedules.ghcrWebhookAudit`（必含）。
  - `PUT /api/settings` 支持可选 `schedules`，且子字段缺省时保持原值。
  - 当 `enabled=true` 时，`cron` 必须可解析；非法返回 `400 invalid_argument`，并带 `details.reason = "cron_invalid"`。
- Cron 表达式：
  - 支持 5 段（min hour dom mon dow）与 6/7 段（sec min hour dom mon dow [year]）。
  - 5 段表达式解析时自动补全秒字段为 `0`（等价每次在 00 秒触发）。
  - 解释时区为服务端本地时区（`chrono::Local`），由部署的 `TZ` 决定。
- 任务执行与可见性：
  - 定期检查更新：到点创建 `check` job（`createdBy=schedule`、`reason=schedule`、`scope=all`），并执行与 UI 点击“检查更新”同一套逻辑（check + persist）。
  - webhook 巡查：到点入队 `github_packages_webhook` 的 `audit_all`，并避免重复入队（若已有 schedule 来源的 pending job，跳过本次）。
  - `/queue` 可见 `by schedule · reason schedule`。
- UI 文案：
  - 导航与顶栏不再出现“更新队列”，统一为“任务队列”。

### SHOULD

- Settings 输入框在 `cron_invalid` 时有明确错误提示（toast + inputError）。
- Scheduler 对“已有任务 running”采取跳过策略，避免补跑与任务堆积。

### COULD

- 后续新增“展示下一次触发/上一次触发”与“立即触发一次”的按钮（本期不做）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /api/settings` | HTTP API | external | Modify | None | dockrev-api | web | 新增 schedules 字段 |
| `PUT /api/settings` | HTTP API | external | Modify | None | dockrev-api | web | 新增 schedules 入参与校验 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 打开设置页，When 请求 `GET /api/settings`，Then 响应包含 `schedules.updateCheck` 与 `schedules.ghcrWebhookAudit` 且字段完整。
- Given `PUT /api/settings` 传入 `schedules.updateCheck.enabled=true` 且 cron 非法，
  When 保存，
  Then 返回 `400` 且 `details.reason == "cron_invalid"`，前端显示“Cron 表达式不合法...”。
- Given `updateCheck.enabled=true` 且 cron=`* * * * *`，
  When 等待 1~2 分钟，
  Then `/queue` 中出现 `check` 类型任务，且 `createdBy=schedule`、`reason=schedule`。
- Given `ghcrWebhookAudit.enabled=true` 且 cron=`* * * * *`，
  When 等待 1~2 分钟，
  Then `/queue` 与 GHCR 队列页出现 `github_packages_webhook` 的 `audit_all` 任务（pending/running），且来源为 schedule。
- Given `/queue` 页面与侧栏导航，
  When 查看模块名，
  Then 不再出现“更新队列”，统一显示“任务队列”。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: DB settings 新增 schedule 列 + migrate + defaults
- [x] M2: Settings API types 与 get/put 扩展（含 cron 校验与错误 reason）
- [x] M3: Cron scheduler：定期 check + GHCR audit（替换原 interval audit）
- [x] M4: 容器/部署：tzdata + `TZ=Asia/Shanghai`
- [x] M5: Web：SettingsPage UI + api types + storybook mock 同步
- [x] M6: Web：/queue 模块名统一为“任务队列”
- [x] M7: 测试与回归：`cargo test -p dockrev-api` + `bun test` + `bun run --cwd web build`
