# Contracts

本计划新增/修改的契约文档如下：

- `ui.md`: UI 入口与交互契约（按钮位置、启用条件、确认与错误提示口径、请求参数、自我升级跳转）。
- `http-apis.md`: supervisor 外部 HTTP API（自我升级启动/状态/回滚）。
- `cli.md`: supervisor internal CLI contract（compose/pull/up/healthcheck/rollback）。
- `file-formats.md`: supervisor 状态持久化文件（可恢复）。
- `config.md`: web/supervisor 配置接口（env/flags）。

依赖但不修改的既有契约：

- `POST /api/updates`：见 `docs/plan/0001:dockrev-compose-updater/contracts/http-apis.md`（本计划仅新增 UI 入口，不变更 API 形状）。
