# ADR 0023: Web Demo 模拟身份必须复用产品鉴权语义

## Status

Accepted. 修正 demo inspector 的环境控制设计。

## Context

Dockrev 是单租户运维控制台，鉴权只有两个结果：入口代理（Forward Auth）提供的
用户/组命中 allowlist 就是**已授权**，否则 Dockrev 返回 `401 auth_required` 并
渲染 `UnauthorizedPage`。没有角色层级，没有“只读用户”，也没有“部分功能可用”
的概念——未授权就是全部功能不可访问。

`crates/dockrev-api/src/authz.rs` 的判定顺序是：

1. 用户头命中 `auth_allowed_user` → `matched_by=user`
2. 组头命中 `auth_allowed_group` → `matched_by=group`
3. `allow_anonymous_in_dev` 且未配置 allowlist → `matched_by=anonymous_dev`
4. 未配置 allowlist → `authz_config_missing`
5. 无身份 → `identity_missing`
6. 其他 → `authz_no_match`

前端对 401 的响应是全局阻断：`App.tsx` 的 `authFailure` 一旦非空，整棵 App 树
换成 `UnauthorizedPage`，不渲染任何业务页面。

我在实现 demo inspector 时引入了 `guest` / `user` / `admin` 三档身份，并让
mock 永远不返回 401。这同时违反了三条产品事实：

- 三档身份在真实产品里不存在，`user` 与 `admin` 没有权限差异；
- 未授权的核心后果是**功能完全不可用**，而 demo 里 guest 仍能点完所有按钮；
- 身份切换因此对评审者不可见——看起来“完全没变化”。

## Decision

Demo 的身份控制只暴露产品真实存在的两种状态，并让 mock 真正执行产品鉴权。

### 状态

| 控件值 | 语义 | 产品表现 |
| --- | --- | --- |
| `allowed` | 命中 allowlist 的维护者 | 正常渲染控制台，`matched_by=user` |
| `blocked` | 未命中 allowlist 的访问者 | 所有 `/api/**` 返回 `401 auth_required`，渲染 `UnauthorizedPage` |

- 分享参数沿用 `d_persona`，取值改为 `allowed` / `blocked`。
- 不再保留 `anonymous_dev` 作为可选人格：它是开发环境兜底，不是可演示的身份。
  未配置 allowlist 的部署在 demo 中固定表现为 `allowed`，避免把配置缺失伪装成权限问题。
- 旧的 `guest` / `user` / `admin` 值按映射处理：`admin`、`user` → `allowed`，
  `guest` → `blocked`，保证既有分享链接仍可打开。

### 行为

- `blocked` 时 mock 对所有非公共 API 返回与后端一致的
  `401 {"error":{"code":"auth_required","details":{...}}}`，其中 `details` 复用
  `authz_error` 的字段名（`reason`、`forwardHeaderName`、`allowedUserMasked`、
  `currentUser`、`currentGroups`、`authorizationMode` 等）。
- 公共豁免与产品一致：`GET /api/health`、`GET /api/version`、`/api/webhooks/*`。
- 前端已有的 `AUTH_REQUIRED_EVENT` → `authFailure` → `UnauthorizedPage` 链路
  直接生效，demo 不新增任何阻断逻辑。
- 从 `blocked` 切回 `allowed` 时，模型重建让 App 重新读取 `/api/settings`，
  控制台恢复。这与真实用户被加入 allowlist 后的恢复路径同构。

### Inspector 文案

- 控件标题为「模拟鉴权」，选项为「已授权（命中 allowlist）」「未授权（未命中）」。
- 面板不得声称存在管理员特权；`SIMULATED` 状态条继续标明 mock-only。

### 离线与鉴权的交互

- `blocked` 优先于网络的可用性：未授权的会话不应先看到业务骨架再看到错误。
- 离线状态下已渲染的未授权页保持不变，恢复在线不改变鉴权结论。

## Consequences

- Demo 能证明“未授权等于完全不可用”这条产品约束，而不是把它藏起来。
- 评审者看到的身份切换是可见的：控制台与 401 阻断页之间切换。
- 放弃三档人格，意味着 demo 无法演示“角色差异”——因为产品本身没有角色差异。
  未来若引入真实角色，需要新的 ADR，而不是让 demo 先假设。
- `d_persona` 取值变化会让旧分享链接改变行为；映射表保证它们不会 404 或白屏。
- 工作量集中在 mock 的鉴权响应与 inspector 的两个选项；前端阻断链路保持不动。

## Verification

- `blocked` 时 `GET /api/stacks` 返回 401 且页面渲染 `UnauthorizedPage`。
- `blocked` 时 `GET /api/health`、`GET /api/version` 仍返回 200。
- `allowed` 时控制台正常加载，左上角身份显示命中方式。
- 切换 `blocked → allowed` 后无需整页刷新即恢复控制台。
- 旧链接 `d_persona=guest` 表现为 `blocked`，`d_persona=admin` 表现为 `allowed`。
