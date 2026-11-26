---
title: 公开 Demo
description: Dockrev 独立 Web Demo 站点的场景、分享参数与版本语义。
---

# 公开 Demo

公开 Demo 是**独立站点**：<https://dockrev-demo.ivanli.cc/>。它不是文档站子目录，也不连接任何真实 Dockrev 后端。

## 它是什么

- 复用正式 React app 的页面、路由与状态模型，数据来自浏览器内的 mock 层。
- 所有写操作只改内存模型，并在 Demo 控制面板的“模拟写入”区留痕。
- 版本号在构建时确定：页面显示的是这份产物构建时的版本，`/api/version` 回读同一个值，不查询真实服务。

## 场景参数

URL 决定场景与可复现状态：

| 参数 | 作用 | 取值 |
| --- | --- | --- |
| `demo` | 选择场景 | `overview`、`services`、`service-actions`、`service-versions`、`queue`、`queue-logs`、`settings`、`ghcr-registry`、`cleanup`、`deploy-check` |
| `d_persona` | 模拟鉴权 | `allowed`、`blocked` |
| `d_connection` / `d_delay` | 网络与延迟（一个三态控件写入这两个字段） | `online`/`offline` + `normal`/`slow`/`custom` |
| `d_delay_ms` | 自定义附加延迟（毫秒） | `0`–`30000` 整数，仅在 `custom` 生效 |
| `d_motion` | 动效偏好 | `system`、`reduce` |
| `d_case` | 异步数据态 | `default`、`cold`、`cache-refresh`、`error`、`job-detail-retry` |
| `d_events` | 管理事件流 | `default`、`reconnecting` |
| `d_cleanup` | 清理场景 | `cleanup-console-storage-normal`、`cleanup-console-confirm-pending`、`cleanup-console-confirm-failed` |
| `d_controls` | 隐藏控制面板 | `hidden` |

默认值不写进 URL。例如 `https://dockrev-demo.ivanli.cc/services?demo=services&d_delay=slow` 只记录与默认值不同的 `d_delay`。旧链接的 `guest` 视为 `blocked`，`user` / `admin` 视为 `allowed`。

## Demo 控制面板

- 桌面：右侧（或左侧）浮窗，可收起成 `DEMO` 气泡，位置按场景分别记忆。
- 移动端：右下角 `DEMO` 气泡展开为底部 drawer。
- `全局环境` 常驻：模拟鉴权、网络与延迟、产品主题。鉴权与主题复用产品自己的会话与主题模型，不是第二套状态。
- 网络与延迟是一个 **三态** 控件：正常 / 慢速 / 离线。自定义延迟在 `高级` 中启用后，控件下方显示当前自定义值。
- `未授权` 时所有非公共 API 返回 `401 auth_required`，页面切换为「当前身份未获 Dockrev 授权」，控制台完全不可用；切回 `已授权` 无需刷新即恢复。
- 面板提供场景切换、数据、Actions、分享链接与模拟写入记录。场景切换是 client-side 导航，不会整页刷新。
- 需要干净截图时使用 `d_controls=hidden`。

## 与 Storybook 的关系

Storybook 是组件与页面状态的 QA 图库，不替代产品演示面；Demo 提供的是可点击、可分享的真实产品路径。

## 发布与配置

- 只有 `Release` 会构建并发布 demo 站点；PR 只做 build + smoke。
- 首次启用需要 owner 在 EdgeOne Makers 建好 direct-upload 项目与域名绑定，并配置仓库变量 `DOCKREV_DEMO_PUBLISH_ENABLED=true`、secret `EDGEONE_API_TOKEN` 与 `DOCKREV_DEMO_EDGEONE_PROJECT`。未启用时 Release 仍产出 `dockrev-demo-web-<version>` artifact，可手工部署。
