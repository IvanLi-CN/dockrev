# UI Contract: One-click apply actions + self-upgrade entry

目标：

- 让登录用户从 UI 直接触发 `POST /api/updates` 的 `mode=apply`，覆盖三种 scope（service/stack/all），并在错误时给出可行动提示。
- 当目标 service 为 Dockrev 自身时，“升级”必须跳转到自我升级页面（独立于 Dockrev 生命周期）。

## 入口与位置

### 全部更新（scope=all）

- 位置：Overview 页顶栏 actions。
- 文案：`更新全部`（或等价中文，需与现有 UI 语言一致）。

### 更新某个 stack（scope=stack）

- 位置：Overview 页与 Services 页中，“stack 分组头部”区域（与该组的折叠/展开、摘要信息同一行）。
- 文案：`更新此 stack`（或等价中文）。

### 更新某个 service（scope=service）

- 位置：
  - Service detail 页顶栏 actions
  - Overview/Services 的 service 行（快速入口）
- 文案：
  - 普通服务：`执行更新`
  - Dockrev 自身：`升级 Dockrev`（点击跳转自我升级页面）

### 自我升级页面（Dockrev self-upgrade）

- 入口：
  - 在列表/详情对 Dockrev 服务点击“升级 Dockrev”后跳转
  - 在系统设置页提供固定入口（便于找回）
- 目标：在 Dockrev 重启窗口内仍可用，因此该页面应由 supervisor/agent 提供（见 `contracts/http-apis.md`）。
- 跳转 URL：来自 `DOCKREV_SELF_UPGRADE_URL`（默认 `/supervisor/`；见 `contracts/config.md`）。

## 启用/禁用规则

- 通用：
  - busy（已有请求进行中）时禁用。
  - 若 UI 无法确定“是否可更新”（例如未扫描/缺少 candidate/仅有 hint），也应允许触发，但必须在确认文案中提示“将由服务端按候选版本/忽略规则计算是否实际变更”。
- service scope：
  - 三态规则（必须按 UI 能拿到的信息做判断）：
    - 已知可更新（updatable=true）：启用。
    - 已知不可更新（updatable=false 且原因明确，例如无候选/架构不匹配/被阻止）：禁用并显示原因。
    - 未知（无法判断 updatable，例如未扫描/缺少 candidate/仅有 hint）：启用，并在按钮附近显示“未确认是否有更新；将由服务端计算”。
- stack / all scope：
  - 聚合规则（对范围内所有 services 汇总）：
    - 若至少存在 1 个“已知可更新”：启用。
    - 若不存在“已知可更新”，但存在至少 1 个“未知”：启用，并显示“存在未扫描/未知的服务；将由服务端计算是否实际变更”。
    - 若全部为“已知不可更新”：禁用并显示“无可更新服务”（可补充 1 个主要原因摘要，如“无候选”）。
- Dockrev 自身：
  - 不走 `POST /api/updates`；不要求 `updatable` 才可点击（由 supervisor 自行判断可否升级/是否有新版本）。
  - Dockrev UI 必须先探测 supervisor 可用性（`GET /supervisor/health`）：
    - ok：允许点击并跳转
    - not ok：禁用入口并显示“自我升级不可用（supervisor offline）”，提供重试检查入口

## 交互（最小确认）

- 点击后必须二次确认（允许使用 `window.confirm` 作为 MVP）：
  - 展示 scope（all/stack/service）
  - 展示目标（stack 名 / service 名）
  - 展示风险提示（会拉取镜像、重启容器，失败可能回滚）
- 确认后才允许发送请求。

## 请求契约（POST /api/updates）

### Request body

固定字段（所有入口一致）：

```json
{
  "mode": "apply",
  "allowArchMismatch": false,
  "backupMode": "inherit",
  "reason": "ui"
}
```

scope 与目标字段：

- all:
  - `{ "scope": "all" }`
- stack:
  - `{ "scope": "stack", "stackId": "<stk_...>" }`
- service:
  - `{ "scope": "service", "stackId": "<stk_...>", "serviceId": "<svc_...>" }`

### Success response

```json
{ "jobId": "job_..." }
```

### Error handling

- `401`：提示“需要登录/鉴权（forward header）”，并建议检查反代配置。
- `409`：提示“该 stack 正在更新”，并引导用户去“更新队列”查看。
- 其它错误：展示 `message`（或 fallback 为字符串化错误），并保留重试入口（再次点击）。

## 成功后的引导

- 必须展示 `jobId`（toast/inline message 均可）。
- 必须提供进入“更新队列”的入口（导航到 `/queue` 即可）。

## 自我升级（Dockrev 更新 Dockrev）

### 行为

- 当用户点击 Dockrev 服务的“升级 Dockrev”：
  - 必须跳转到自我升级页面（supervisor console）。
  - 禁止调用 `POST /api/updates`。

### 自我升级页面职责

- 展示当前 self-upgrade 状态（轮询 `GET /supervisor/self-upgrade`）。
- 允许发起升级/预览/回滚（调用 `POST /supervisor/self-upgrade` 与 `POST /supervisor/self-upgrade/rollback`）。
- Dockrev 恢复后提供“返回 Dockrev”入口（例如链接到 `/`）。

### Dockrev 自身识别

- 采用“自动匹配 + 支持配置覆盖”的策略：
  - 自动匹配：按镜像仓库 `ghcr.io/ivanli-cn/dockrev`（或其配置值）匹配
  - 覆盖：支持显式配置指定目标容器
- 具体配置项见 `contracts/config.md`。

## Supervisor offline UX

目标：当 Dockrev UI 可用但 supervisor 不可用时，用户应得到明确反馈且不进入“浏览器 502 空白页”。

- 探测方式：`GET /supervisor/health`（短超时，允许缓存最近一次结果）。
- 失败态展示（Dockrev 服务行与详情均需一致）：
  - 入口按钮 disabled
  - 辅助文案：`supervisor offline`（包含最近一次探测时间/错误摘要，如可得）
  - 提供 `重试` 操作：重新发起 health 探测
