# GitHub/GHCR: 自动注册 `package` webhook（新镜像发布通知）（#hkr8b）

## 状态

- Status: 待实现
- Created: 2026-01-30
- Last: 2026-01-30

## 背景 / 问题陈述

- 现状：Dockrev 以轮询/手动触发为主，无法在 GHCR 发布新镜像版本时“主动唤醒”扫描与通知。
- 目标：在 GitHub Packages（GHCR）发生新版本发布时，GitHub 主动回调 Dockrev，从而触发扫描与通知。
- 约束：需要通过 GitHub API 自动注册 webhook；同时在 Web UI 上可配置目标与凭据，且 PAT 不得被回传给前端（只能掩码展示）。

## 目标 / 非目标

### Goals

- 在 Web UI 的 Settings 中新增 “GitHub Packages (GHCR) Webhook” 配置区：
  - 支持输入：仓库 URL / profile URL / 用户名，并自动识别类型与解析 owner/repo。
  - 可拉取并展示该 owner 下的 repo 列表（checkbox，默认全选）。
  - 支持输入 GitHub PAT（仅用于后端调用 GitHub API；GET 时永不返回明文，只返回掩码）。
  - 支持配置回调地址（callback URL）；默认用浏览器当前 URL 推算。
- 支持同时配置多个 owner（多个 targets）；repos 勾选集合允许跨 owner。
- 后端支持按用户选择的 repos 自动注册 GitHub webhook（订阅 `package` 事件），并保证幂等：
  - 同一 repo 重复 “保存/同步” 不会创建重复 webhook。
  - 若 repo 中已存在目标 webhook，则复用之并更新配置（必要时）。
- 在收到 GitHub webhook 投递后：
  - 验证签名（`X-Hub-Signature-256`），并校验事件为 `package` + `action=published`。
  - 对命中的 repo 触发一次 discovery scan（Job），并复用现有通知机制发出消息（作为 job 更新通知的一部分）。

### Non-goals

- GitHub App / OAuth 登录（本计划只用 PAT）。
- Org-level webhook（`POST /orgs/{org}/hooks`）与 org owner 权限链路（本计划按 repo 级 webhook 为主）。
- 默认自动删除/清理 repo 中旧的 webhook（必须由用户确认后才执行删除）。
- 为所有 registry（Docker Hub/Quay/Harbor 等）提供 webhook 统一抽象（仅覆盖 GitHub Packages / GHCR）。

## 范围（Scope）

### In scope

- 新增 GitHub Packages webhook 集成配置的持久化（DB schema 变更设计见契约）。
- 新增后端 API：
  - 解析输入并列出 repos（用于 UI 勾选）。
  - 读取/更新配置（PAT 掩码策略）。
  - 同步 webhook（幂等创建/更新/复用）。
  - 冲突处理：当发现重复 webhook 时，支持在用户确认后删除旧的 webhook。
  - 接收 webhook（签名校验 + 事件过滤 + 触发 job）。
- Web UI（Settings）：
  - 新增配置区、targets 列表、repo 勾选表、PAT 输入掩码、callback URL 默认推算与可编辑。

### Out of scope

- 部署层面的公网可达性、TLS、反向代理配置（由 deployer 负责）。
- 无 token 条件下的“仅 public repo”弱功能（为减少分支，本计划要求配置 PAT 才能完成解析+同步）。
- 后台定期校验/自动修复 “repo webhook 配置漂移”（仅提供手动 “同步 webhook”）。

## 需求（Requirements）

### MUST

- UI 支持输入一个或多个 `targets`（repo URL / profile URL / username）并在保存前可预览解析结果与 repo 列表（默认全选）。
- UI 支持输入 GitHub PAT：
  - GET 配置时返回 `******` 掩码或空值；不得返回真实 PAT。
  - PUT 配置时允许提交 `******` 表示“不修改既有 PAT”。
  - 后端日志/错误中不得打印 PAT。
- 要求先配置 PAT：未保存 PAT 时，不允许解析 profile/username，也不允许同步 webhook（UI 与后端均需做校验与错误提示）。
- UI 支持配置 callback URL：
  - 默认值为 `new URL('/api/webhooks/github-packages', window.location.origin).toString()`。
  - 用户可手动覆盖。
- “同步 webhook” 幂等：
  - 对每个选中 repo，在创建前先 `list hooks` 并匹配 `config.url==callbackUrl` 且订阅包含 `package` 的 webhook：
    - 若已存在：复用（noop 或仅做必要更新）。
    - 若不存在：创建新的 webhook。
- 重复 webhook 冲突处理（用户确认后才能删除）：
  - 若 `list hooks` 匹配到多个：返回 `conflict`，并提供候选 hook 列表（id + url + events + active + updated_at 摘要）。
  - UI 必须询问用户是否要删除旧的（保留 1 个作为“目标 hook”，删除其余重复项）。
  - 用户确认删除后，再次执行 sync 时可以携带明确的删除指令（目标 repo + keep_hook_id + delete_hook_ids）。
- webhook 接收端必须验证签名 `X-Hub-Signature-256`，签名失败返回 401。
- webhook 接收端必须过滤事件：只处理 `X-GitHub-Event=package` 且 payload 中 `action=published` 的事件。
- webhook 投递具备“至少一次”语义：必须使用 `X-GitHub-Delivery` 做去重（TTL/窗口由实现定义），避免重复触发大量扫描。
- webhook 命中后触发一次 `discovery` job（scope=all），并将触发来源写入审计字段（created_by/reason）。
- repo 列表合并规则：
  - 当多个 targets 解析后出现重复 repo（同 `owner/repo`），必须去重；去重后仍保持 `selected` 的确定性（若任一来源为选中，则视为选中）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| GitHub Packages webhook settings APIs | HTTP API | internal | New | ./contracts/http-apis.md | Dockrev | Web UI | Settings 页面配置与同步 |
| GitHub `package` webhook delivery | Event | external | New | ./contracts/events.md | GitHub | Dockrev API | `action=published` |
| GitHub Packages webhook persistence | DB | internal | New | ./contracts/db.md | Dockrev | Dockrev API | PAT 掩码；hook_id 持久化；delivery 去重 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/events.md](./contracts/events.md)
- [contracts/db.md](./contracts/db.md)

## 验收标准（Acceptance Criteria）

- Given 管理员打开 Settings 页面
  When 未配置 GitHub webhook 集成
  Then 页面展示 target 输入框、PAT 输入框（为空）、callback URL（按当前浏览器 origin 推算的默认值）、repo 列表为空且提示“先解析”

- Given 管理员输入一个 repo URL 并点击“解析”
  When 后端完成解析
  Then repo 列表仅包含该 repo 且默认勾选

- Given 管理员输入一个 profile URL 或 username 并点击“解析”
  When 后端使用 PAT 拉取 repo 列表
  Then repo 列表展示该 owner 下的 repos，且默认全选

- Given 管理员先后解析并加入两个不同 owner 的 targets
  When 页面展示 repo 勾选列表
  Then repo 列表可跨 owner 勾选（默认全选），且保存后仍能完整回显

- Given 已保存过 PAT
  When 再次打开 Settings 页面
  Then PAT 输入框显示为掩码（例如 `******`），且任何 GET 接口响应不包含 PAT 明文

- Given 已保存配置并完成一次“同步 webhook”
  When 再次点击“同步 webhook”
  Then 对每个已选中 repo 的结果为 `noop`（或仅做必要 update），GitHub 上不会出现重复 webhook

- Given 某 repo 已存在多个与 callback URL 相同且订阅 `package` 的 webhook
  When 点击“同步 webhook”
  Then UI 展示冲突详情并询问是否删除旧的 webhook

- Given 某 repo 已存在多个重复 webhook 且用户确认“删除旧的”
  When 再次执行“同步 webhook”（携带 keep/delete 指令）
  Then Dockrev 删除重复项，仅保留 1 个目标 webhook，并在后续同步中保持幂等

- Given GitHub 发送 `package` webhook 到 Dockrev
  When 签名无效
  Then Dockrev 返回 401 且不触发任何 job

- Given GitHub 发送 `package` webhook（`action=published`）到 Dockrev
  When 签名校验通过且 repo 在选中集合中
  Then Dockrev 创建一个 `discovery` job（scope=all），并记录触发来源为 GitHub webhook

- Given GitHub 重复投递同一 delivery（相同 `X-GitHub-Delivery`）
  When Dockrev 接收该投递
  Then 仅触发一次 discovery job（其余请求 2xx 返回但不重复触发）

## 实现前置条件（Definition of Ready / Preconditions）

- 本计划的接口契约已定稿（contracts 已冻结），实现与测试可直接按契约落地
- 已确认 GitHub webhook 事件使用 `package`（并以 `action=published` 为触发条件）
- 已确认 callback path：`/api/webhooks/github-packages`

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests（Rust）:
  - `target` 输入解析（repo/profile/username + 常见变体）。
  - webhook 幂等匹配逻辑（基于 `config.url` + `events` 组合）。
  - `X-Hub-Signature-256` 校验与 delivery 去重。
- Integration tests（Rust, API tests）:
  - settings GET/PUT 掩码策略（`******` merge_secret 语义）。
  - sync endpoint 在 “已存在 hook / 不存在 hook / 多个 hook” 下的行为（GitHub client 需可 mock）。
  - webhook receiver：签名失败/成功 + job 创建断言。

### UI / Storybook (if applicable)

- 在 Settings 相关 mocks 中补齐 GitHub webhook 配置数据形状（用于 Storybook 快速回归）。

### Quality checks

- 通过仓库既有的 Rust tests + TypeScript typecheck（不引入新工具链）。

## 文档更新（Docs to Update）

- `README.md`: 增补 “GitHub Packages (GHCR) webhook” 的使用说明、最小权限 PAT 指引、以及公网可达性提示。
- `docs/plan/README.md`: 本计划进度更新（由实现阶段推动）。

## 计划资产（Plan assets）

- Directory: `docs/plan/hkr8b:github-package-webhook-registration/assets/`
- In-plan references: `![...](./assets/<file>.png)`

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones）

- [ ] M1: 后端 settings + resolve + DB 契约落地（含 PAT 掩码策略）
- [ ] M2: webhook sync（GitHub API 客户端 + 幂等逻辑 + 冲突处理）
- [ ] M3: webhook receiver（签名校验 + delivery 去重 + 触发 discovery job）
- [ ] M4: Web UI Settings 配置区（repo 勾选 + PAT 掩码 + callback URL 默认值 + 同步按钮）
- [ ] M5: 测试补齐 + README 使用说明

## 方案概述（Approach, high-level）

- 以 repo webhook 为单位注册：对用户选择的 repos 逐个创建/复用 webhook，事件仅订阅 `package`。
- webhook secret 由服务端生成并持久化：UI 不展示明文；创建 webhook 时写入 GitHub config。
- 幂等：
  - 以 GitHub 现有 hooks 列表 + 本地持久化 hook_id 双重判定，确保重复操作不产生重复 webhook。
  - 对 delivery 做短期去重，避免重复投递引起重复扫描。
- 触发逻辑：
  - webhook receiver 只做轻量校验与入队（job），不做重计算；扫描与通知复用既有 job/notify 体系。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：
  - GitHub `package` webhook payload 中 `package.repository` 可能缺失（例如 package 未绑定到 repo），需以“可容错的 owner/repo 提取 + 合理降级触发”为准并做好兼容解析。
  - PAT 权限不足/组织策略限制可能导致无法创建 webhook，需要 UI 提示明确的失败原因。

## 变更记录（Change log）

- 2026-01-30: 创建计划。
 - 2026-01-30: 支持多 owner targets；重复 webhook 由用户确认后可删除；要求先配置 PAT。

## 参考（References）

- GitHub Webhook events: `package`（GitHub Docs）。
- GitHub REST API: create/list repository webhooks（`/repos/{owner}/{repo}/hooks`）。
- GitHub webhook signature verification（`X-Hub-Signature-256`）。
