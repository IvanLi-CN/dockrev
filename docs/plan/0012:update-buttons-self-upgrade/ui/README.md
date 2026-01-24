# UI 设计说明

本目录包含 Plan 0012 的 UI 设计图与交互说明，作为实现阶段的唯一口径。

## 页面与入口

- Dockrev UI
  - Overview：`更新全部`（apply）+ 每个 stack 的 `更新此 stack`（apply）+ service 行的 `执行更新`（apply）
  - Services：同上（stack 头 + service 行）
  - Service detail：`执行更新`（apply）
- 自我升级（supervisor 页面）
  - Dockrev 服务在列表/详情的入口为 `升级 Dockrev`，点击后跳转到 `DOCKREV_SELF_UPGRADE_URL`（默认 `/supervisor/`）

## 关键交互

### apply 更新（普通服务/stack/all）

- 点击 → 最小确认（scope + 目标 + 风险提示）→ 调用 `POST /api/updates`（mode=apply）→ 显示 `jobId` → 引导到 `/queue`。
- `401`：提示需要登录（forward header）。
- `409`：提示 stack 正在更新，并引导去队列查看。

### 自我升级（Dockrev）

- 点击 `升级 Dockrev` 前必须先探测 supervisor：`GET /supervisor/health`
  - ok：跳转 `/supervisor/`
  - not ok：入口禁用 + 显示 `supervisor offline` + 提供 `重试`（重新探测）
- 自我升级页面轮询 `GET /supervisor/self-upgrade` 展示状态；允许：
  - `预览`（dry-run）
  - `开始升级`（apply）
  - `回滚`（失败时尝试回滚；也允许手动触发）
- 页面需要明确展示：
  - 当前 Dockrev 版本（若 Dockrev 暂不可用则显示 unknown）
  - 目标版本（tag/digest）
  - 当前 step（pull/apply/wait_healthy/rollback）
  - 最新错误与日志摘要

## supervisor 不可用时的反馈

目标：避免用户从 Dockrev UI 跳转后直接遇到 502 空白页。

- Dockrev UI 必须在入口处显示 supervisor 可用性：
  - 可用：按钮可点
  - 不可用：按钮禁用，显示“自我升级不可用（supervisor offline）”，提供重试

## 设计图

- `overview-update-actions.svg`：Overview 的 all/stack/service 入口布局（含 stack 级入口、service 行入口、Dockrev 自身“升级 Dockrev”与 offline 态）
- `service-detail-update-actions.svg`：Service detail（Dockrev 自身跳转入口、health probe、offline 重试）
- `system-settings-self-upgrade.svg`：系统设置页固定入口（避免 Dockrev 服务不可见时无法进入），含 supervisor offline 反馈
- `self-upgrade.svg`：自我升级页面（running 状态示例）
- `self-upgrade-offline.svg`：自我升级页面（offline 状态：状态 API 不可达 + Retry + last known）
- `components-self-upgrade-status.svg`：自我升级组件多状态集中图（status banner / controls / health widget）
