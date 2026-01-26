# Dockrev Web: 一键执行更新（service/stack/all）+ 自我升级策略（#0012）

## 状态

- Status: 已完成
- Created: 2026-01-24
- Last: 2026-01-25

## 背景 / 问题陈述

- 现状：Web UI 仅支持“扫描更新”（check）与“预览更新”（dry-run），缺少 `mode=apply` 的入口，无法从 UI 直接执行更新。
- 影响：登录用户需要绕过 UI（手工调用 API / 外部系统触发 / 自己跑 compose）才能升级，审计与操作路径不一致。
- 额外风险：Dockrev 作为被管理对象时会出现“自我升级”（更新正在运行的 Dockrev 容器）。为保证可靠性，自我升级必须有单独界面与独立执行者，避免更新过程被 Dockrev 自身重启中断。

## 目标 / 非目标

### Goals

- 在 UI 增加三类“执行更新（apply）”入口：服务级（service）、stack 级（stack）、全部（all）。
- 登录用户点击后可直接创建 update job，并能在“更新队列”中查看 job 状态与日志。
- 自我升级（Dockrev 更新 Dockrev）提供单独界面：从服务列表/详情点击“升级 Dockrev”后跳转到自我升级页面。
- 自我升级流程必须可靠：升级执行与状态展示在 Dockrev 重启窗口内仍可持续可见、可恢复、可回滚（如适用）。

### Non-goals

- 不引入复杂权限模型（RBAC/多角色/多租户）。
- 不实现自动化更新策略语言/定时自动更新（计划另议）。
- 不在本计划内默认开放 `allowArchMismatch=true`（仅讨论默认行为与 UI 暴露策略）。

## 范围（Scope）

### In scope

- Web UI：新增 apply 更新按钮与最小必要的交互（确认、busy、错误提示、引导到队列）。
- Web UI：新增“自我升级”独立页面，并从 Dockrev 服务的“升级”入口跳转到该页面。
- 自我升级：实现一个独立执行者（supervisor/agent，独立于 Dockrev 容器生命周期）来完成 Dockrev 自身升级，并提供可轮询的状态 API 与独立页面（用于 Dockrev 重启期间持续可用）。
- 文档：补齐“如何从 UI 执行更新 / 如何自我升级”的最小操作说明（含鉴权前提）。

### Out of scope

- 不新增 Dockrev 本体的 `/api/*` 端点（优先复用 `POST /api/updates` 既有契约；supervisor 的 `/supervisor/*` API 不在此限制内，见 In scope 与契约）。
- 为其它服务更新引入宿主机 agent/sidecar（本计划的 agent 仅服务于自我升级；扩展为通用执行者另起计划）。

## 需求（Requirements）

### MUST

- UI 必须提供 3 个 apply 入口：
  - 全部更新（scope=`all`）
  - 更新某个 stack（scope=`stack` + `stackId`）
  - 更新某个 service（scope=`service` + `stackId` + `serviceId`）
- service 级 apply 入口必须同时存在于：
  - Service detail 页
  - Overview/Services 的 service 行（或等价的快速入口）
- 触发更新时 UI 调用 `POST /api/updates`，请求体固定为：
  - `mode="apply"`
  - `allowArchMismatch=false`
  - `backupMode="inherit"`
  - `reason="ui"`
- UI 必须在触发前提供最小确认（避免误触）；确认内容至少包含：scope、目标（stack/service 名称）与“可能触发回滚/重启”的提示。
- UI 必须处理关键错误：
  - `401`：提示需要登录（forward header）
  - `409`：提示该 stack 正在更新（禁止并发）
  - 网络/其它错误：可读错误提示，不应静默失败
- UI 必须在触发成功后给出可追踪信息（至少展示 `jobId`），并提供进入“更新队列”的引导入口。
- 自我升级：当目标 service 识别为 Dockrev 自身时，“升级”动作不得走 `POST /api/updates`，必须跳转到自我升级页面。
- 自我升级：自我升级页面必须在 Dockrev 重启窗口内仍可用（因此需要独立于 Dockrev 的执行者与页面/接口）。
- 自我升级：必须具备“可恢复”的状态展示（页面刷新/短暂断连/进程重启后可继续显示进度），并在失败时**尝试回滚**到 previous digest，并给出可行动的重试/人工介入路径。
- 自我升级：Dockrev UI 的“系统设置”页必须提供固定入口到自我升级页面（避免 Dockrev 服务被归档/过滤时无法进入）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| One-click apply actions（all/stack/service） | UI Component | internal | Modify | ./contracts/ui.md | web | end-user | 复用 `POST /api/updates` |
| Self-upgrade UI (supervisor console) | UI Component | external | New | ./contracts/ui.md | supervisor | end-user | Dockrev 自我升级独立页面 |
| Supervisor HTTP API | HTTP API | external | New | ./contracts/http-apis.md | supervisor | web | 自我升级启动/状态轮询/回滚 |
| Supervisor CLI runner | CLI | internal | New | ./contracts/cli.md | supervisor | supervisor | 执行 compose/pull/up/healthcheck |
| Supervisor state file | File format | internal | New | ./contracts/file-formats.md | supervisor | supervisor | 持久化自我升级状态（可恢复） |
| Supervisor config | Config | external | New | ./contracts/config.md | supervisor | deploy | env/flags：识别 Dockrev、basePath、docker/compose 入口 |
| Web config | Config | external | New | ./contracts/config.md | web | deploy | 自我升级入口 URL（默认 `/supervisor/`） |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/ui.md](./contracts/ui.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/cli.md](./contracts/cli.md)
- [contracts/file-formats.md](./contracts/file-formats.md)
- [contracts/config.md](./contracts/config.md)

## 验收标准（Acceptance Criteria）

- Given 用户已登录（forward header 生效），且目标服务状态为“可更新”
  When 点击服务级“执行更新（apply）”并确认
  Then 创建 update job（返回 `jobId`），且用户可在“更新队列”中看到该 job（含 createdBy/reason/logs）
- Given 用户已登录，且某 stack 下至少存在一个“可更新”的服务
  When 点击该 stack 的“更新此 stack（apply）”并确认
  Then 创建 update job（scope=stack），且 UI 展示/引导到队列查看
- Given 用户已登录，且系统存在“可更新”的服务
  When 点击“更新全部（apply）”并确认
  Then 创建 update job（scope=all），且 UI 展示/引导到队列查看
- Given 用户未登录（未注入 forward header）
  When 点击任一 apply 入口
  Then UI 提示需要登录，且不应误导为“已开始更新”
- Given 同一 stack 已存在 running update job
  When 再次触发该 stack 的更新（stack scope 或 all scope 涉及该 stack）
  Then `409` 时 UI 给出明确提示，并且不会重复创建任务
- Given 目标 service 被识别为 Dockrev 自身
  When 在列表或详情点击“升级”
  Then 浏览器跳转到自我升级页面（独立于 Dockrev UI），并且不调用 `POST /api/updates`
- Given 自我升级开始执行，Dockrev 容器需要重启
  When Dockrev 在升级过程中短暂不可用
  Then 自我升级页面仍可持续展示进度，并在 Dockrev 恢复后可引导返回 Dockrev UI
- Given 自我升级失败（pull/up/healthcheck 超时或 unhealthy）
  When supervisor 执行回滚（如适用）
  Then 自我升级页面展示失败原因与回滚结果，并提供“查看日志/重试”入口
- Given Dockrev UI 可用但 supervisor 不可用（`GET /supervisor/self-upgrade` 非 2xx（含 401）或网络失败）
  When 用户查看 Dockrev 服务的“升级 Dockrev”入口
  Then UI 显示“自我升级不可用（supervisor offline）”并禁用该入口，且提供“重试检查/查看说明”的可行动提示
- Given Dockrev 服务在列表中不可见（被归档/被过滤/未展开）
  When 用户进入“系统设置”并点击“自我升级”
  Then 浏览器跳转到自我升级页面（或在 supervisor offline 时给出明确反馈与重试）
- Given 用户已在自我升级页面，且升级进行中
  When supervisor 进程重启或页面刷新
  Then 页面恢复到同一 `opId` 的最新状态（读取状态文件 + 状态 API），不丢失关键进度与日志

## 实现前置条件（Definition of Ready / Preconditions）

- 自我升级部署形态已冻结（supervisor 作为容器还是宿主机二进制；以及入口 URL/路由映射）。
- Dockrev 自身识别口径已冻结（配置优先，避免仅靠 service name/image 猜测）。
- supervisor 如何定位 Dockrev 的 compose（project/service/composeFiles）已冻结，并写入 `contracts/config.md`（含“自动发现 + 可配置覆盖”的优先级）。
- 三类按钮的落点与交互已冻结（见 `contracts/ui.md`），确认方案与错误提示口径不引入新依赖。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Rust: `cargo test -p dockrev-api` 必须通过（不要求新增后端接口测试，但不得破坏现有更新流程测试）。
- Web: `npm run lint`、`npm run build` 必须通过。
- Web: `npm run test-storybook` 必须覆盖“按钮启用/禁用 + 点击后请求参数”两个代表性场景（通过 mock API 校验）。
- Supervisor: 单元/集成测试覆盖“状态持久化 + 幂等重入 + 失败回滚（如适用）”的代表性路径。
  - 必测：supervisor offline（health 不可达）时 Dockrev UI 的禁用态与错误提示逻辑（通过 mock）。

### UI / Storybook

- Stories 至少新增/更新：
  - Overview：all/stack 的 apply 按钮可见性与禁用态
  - Services/Overview：service 行 apply 按钮与禁用态
  - Service detail：service apply 按钮与确认/错误提示
  - Self-upgrade：跳转入口与 supervisor 状态展示（mock）

### Quality checks

- 仅使用仓库既有 lint/typecheck/storybook runner；不引入新工具链与依赖。

## 文档更新（Docs to Update）

- `README.md`: 补充“从 UI 扫描/预览/执行更新”的最小操作说明与鉴权前提（forward header）。
- `docs/plan/0001:dockrev-compose-updater/contracts/http-apis.md`: 若该契约与实现存在偏差，在本计划实现落地后同步对齐（避免文档过期）。
- `deploy/README.md`: 增加 supervisor 的最小部署方式与路由/端口说明（实现落地后同步）。
- `docs/plan/0012:update-buttons-self-upgrade/ui/README.md`: 自我升级页面与按钮入口的 UI 说明（本计划提供）。
- `docs/plan/0012:update-buttons-self-upgrade/ui/*.svg`: UI 设计图（本计划提供）。

## 实现里程碑（Milestones）

- [x] M1: Web UI 增加 all/stack/service 的 apply 更新入口（含确认、busy、错误提示、队列引导）
- [x] M2: Web UI 增加自我升级页面与跳转入口（Dockrev service → self-upgrade page）
- [x] M3: Supervisor/agent 落地：自我升级执行、状态 API、状态持久化与恢复、失败处理（含回滚策略）
- [x] M4: Storybook stories 与 test-storybook 覆盖新增入口（含参数断言）+ 文档同步（README/deploy）

## 方案概述（Approach, high-level）

- 复用既有 `POST /api/updates`，只在前端增加入口与 UX；后端保持接口稳定。
- “stack” 与 “all” 入口放在 Overview/Services 的聚合视图（仓库当前无 stack detail route）。
- 自我升级必须由独立执行者（supervisor/agent）完成：负责 pull/up/healthcheck、状态持久化、失败回滚（如适用），并提供独立页面用于 Dockrev 重启期间持续可见。
  - 默认方案：新增 `dockrev-supervisor`（容器或宿主机进程均可），通过反代将 `/supervisor/` 路由到该服务（从而 Dockrev 宕机/重启时自我升级页面仍可用）。
  - compose 定位建议：优先通过 Docker inspect 读取 Dockrev 容器的 compose labels（`com.docker.compose.project`、`com.docker.compose.project.config_files`）完成自动发现；当 label 不存在或路径不可读时，要求显式配置覆盖。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：
  - 自我升级可能导致更新 job 在执行中被自身重启打断（状态/审计不完整）。
  - all 更新可能被误用；需要最小确认与清晰范围提示。
- 需要决策的问题：
- 假设（需主人确认）：
  - 任意通过 forward header 认证的用户都可执行更新（无角色区分）。【已确认】
  - service/stack/all 的 apply 入口同时覆盖：Service detail + 列表行；Overview + Services 两处聚合入口。【已确认】
  - supervisor 通过反代提供 `/supervisor/` 页面与 API，且该路由独立于 Dockrev 服务可用性。【已确认】
  - 自我升级失败时必须尝试回滚到 previous digest。【已确认】

## 变更记录（Change log）

- 2026-01-24: 创建计划，冻结“新增 UI apply 入口 + 自我升级策略设计”范围与验收草案。
- 2026-01-25: 实现 M1–M4：UI 增加 apply 入口与队列引导；引入 supervisor（/supervisor）用于自我升级；补齐 deploy 与 README；Storybook smoke + 交互断言覆盖。
- 2026-01-25: 修复 UI runtime config 注入的 XSS 风险（转义 `<` 等字符）；Storybook 交互测试改为等待明确条件，降低 CI 抖动。
- 2026-01-25: 修复 Storybook mock 兼容 supervisor 绝对 URL（避免误判 offline）；nginx 增加 `/supervisor` → `/supervisor/` 重定向，避免无尾斜杠落入 Dockrev 反代。
- 2026-01-25: 修复自我升级关键路径：supervisor 目标容器在同镜像多容器场景下可按 compose service 消歧；Dockrev health/version 端口从 `DOCKREV_HTTP_ADDR` 自动推断；`docker compose` 支持绝对路径；UI Dockrev 服务识别改为 repo 前缀匹配。
- 2026-01-25: 自我升级鲁棒性补强：后台任务异常会把状态落盘为 failed（避免卡在 running）；UI 注入 `dockrevImageRepo`（支持 `DOCKREV_IMAGE_REPO`）；docker/compose 超时依赖 `kill_on_drop`，避免遗留进程。
- 2026-01-25: 修复自我升级用户可见问题：rollback 不再在失败时误报成功；supervisor 健康探测改为探测需鉴权的 `/self-upgrade`（避免 UI 误判可用）；空 `DOCKREV_SELF_UPGRADE_URL` 视为未设置。
- 2026-01-25: 自我升级稳定性补强：`docker compose up` 可能重建容器时，health/version 探测会重新解析 target（避免旧 IP 误判失败）；supervisor console 改用 `textContent` 渲染状态/错误，避免 XSS。
- 2026-01-25: 自我升级兼容性修复：当 compose label 的 `config_files` 路径在 supervisor 内不可读时，回退到 `DOCKREV_SUPERVISOR_TARGET_COMPOSE_FILES`；rollback 在无 digest 场景下使用上一次运行镜像引用而不是 `/api/version`，避免构造无效镜像 tag。
- 2026-01-25: 文档修正：UI 启用“升级 Dockrev”的探测口径为 `GET {selfUpgradeBaseUrl}/self-upgrade`，并补充 `DOCKREV_IMAGE_REPO` 的说明。
- 2026-01-25: 体验与容错：`POST /api/updates` 改为创建 job 后后台执行（避免 UI 请求超时）；`useSupervisorHealth` 在 `DOCKREV_SELF_UPGRADE_URL` 配置非法时降级为 offline（不抛未处理异常）。

## 参考（References）

- `web/src/pages/OverviewPage.tsx`
- `web/src/pages/ServicesPage.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/api.ts`
- `crates/dockrev-api/src/api/types.rs`
