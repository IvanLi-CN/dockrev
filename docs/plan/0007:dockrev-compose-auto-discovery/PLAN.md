# Dockrev: Compose 项目自动发现（Auto-Discovery）（#0007）

## 状态

- Status: 待实现
- Created: 2026-01-22
- Last: 2026-01-22

## Change log

- 2026-01-22: 创建计划
- 2026-01-22: 冻结口径：自动发现必启用；移除手动注册；missing 重启自动归档；归档/恢复（project/stack/service）；归档不跳过检查但禁用通知
- 2026-01-22: 补充 UI 设计图（Overview / Archived / Stack detail）

## 背景 / 问题陈述

当前 Dockrev 需要在 UI 手工注册 Compose stack（填写“容器内可见的绝对路径”的 compose 文件）。该流程容易漏注册、路径填错，且新增项目需要额外运维介入。

本计划希望 Dockrev 启动后自动发现本机由 Docker Compose 启动的项目，并自动注册为 Stack，实现“开箱即见”，同时保持鉴权与网络暴露策略不变。

## 目标 / 非目标

### Goals

- Dockrev 启动后自动发现 Compose 项目并自动注册为 Stack（无须手工录入 compose 路径）。
- 安全边界不变：不放宽鉴权、不新增对外暴露端口、不猜测/推断路径。
- 自动发现作为基础能力**必须启用**（无开关）。
- 提供“归档（archive）/恢复（restore）”能力：允许将 stack 或 service 从主视图收纳隐藏，并可在 UI 中恢复。

### Non-goals

- 不自动修改 compose 文件内容。
- 不自动执行更新/回滚/备份（仅发现与注册/同步）。
- 不推断缺失 compose 路径：无可靠证据则不注册。
- 不提供“手动创建 stack（手工注册 composeFiles）”能力（实现阶段将移除相关 UI 与 API）。

## 用户与场景

- 运维/维护者：希望 Dockrev 启动后自动出现现有 Compose 项目，降低“忘记注册”的风险。
- Dockrev 使用者：希望新增一个 `docker compose up -d` 的项目后，在一个扫描周期内自动出现在 UI 中。
- 安全审计：要求 Forward Header 鉴权仍然生效，且 Dockrev 不额外暴露端口。

## 需求（Requirements）

### MUST

- 已确认决策（冻结口径）
  - 项目唯一键：以 `com.docker.compose.project` 作为唯一标识（用于去重与绑定 stack）。
  - 自动发现为基础能力：必须启用（无 enable 开关）。
  - 不支持手动注册 stacks：移除 `POST /api/stacks`，并在 DB migration 清理历史 stack-bound 数据（见 `contracts/db.md`）。
  - missing 判定：一轮未见即 `missing`；进程重启后默认在 UI 主视图隐藏 missing 项（数据仍落库）；项目再次出现时自动恢复展示。
  - 归档语义：不做硬删除；“隐藏/收纳/归档”统一定义为 `archived=true`（可恢复）。
  - 归档范围：允许归档 discovered project、stack、service（不限制 active/missing/invalid）。
  - 归档不跳过扫描/检查：归档的 stack/service 仍参与 check/update 计算，但不会触发通知发送；仅在 UI 显示“有更新的服务数量”。

- 发现来源：通过 Docker Engine API（经 docker-socket-proxy）列出容器，并按 Docker Compose labels 聚合项目：
  - `com.docker.compose.project`
  - `com.docker.compose.project.config_files`（关键：compose 文件路径列表）
  - （可选诊断）`com.docker.compose.project.working_dir`
- `config_files` 解析规则：
  - 支持逗号/换行分隔；去重、去空、trim。
  - 仅接受以 `/` 开头的绝对路径；相对路径一律拒绝（跳过注册/同步并记录原因）。
- 注册/同步策略：
  - 若 `project` 对应 Stack 不存在：创建 Stack
    - `name` 默认等于 `project`（本计划不支持手动改名）。
    - `compose.composeFiles` = 解析后的 `config_files`。
    - `compose.envFile` 不自动推断（可为空）。
  - 若 Stack 已存在：仅当 `config_files` 发生变化且新路径均可读/可解析时，才更新 Stack 的 `composeFiles`。
  - 不支持“手动 stack / 手工注册”的冲突处理（实现阶段将移除手动注册入口）；迁移时需清理历史手动注册数据（见 `contracts/db.md`）。
- 注册前校验（fail-closed）：
  - 每个 compose file 路径存在且可读（容器视角绝对路径）。
  - 文件内容能被 Dockrev 的 compose parser 解析出至少一个可用 service（至少能提取 service 列表/镜像信息）。
- 扫描与生命周期：
  - 后台周期性扫描（默认 `60s`，可配置）。
  - 增量处理：只处理新项目/路径变更/状态变更。
  - 对“已消失项目”（之前发现过、现在无容器）：标记为 `missing`。
    - 在进程未重启前：UI 可见并展示 `missing` 状态标记。
    - 在进程重启后：默认在 UI 主视图隐藏（实现为自动归档：`archived=true`，原因 `auto_archive_on_restart`）。
    - 支持用户手动“归档/恢复”（不做硬删除）；归档后项目再次出现时自动恢复展示。
- UI/运维入口：
  - “无 Stack 的项目”统一聚合到一个组内展示（例如 “Discovered (unregistered)”），而不是每个项目自成一套独立视图/页面。
  - UI 提供“已归档（Archived/归档箱）”入口：可查看并恢复被归档的 discovered projects / stacks / services。
    - archived stacks：按 stack 成组展示（你已确认）
    - archived services：按所属 stack 成组展示（默认，便于恢复；若你希望相反再调整）
  - UI 提供“立即发现/重新同步”按钮；返回摘要（新增/更新/跳过/失败及原因）。
  - 可观测：每轮扫描耗时、发现数、变更数；失败必须有可操作提示（例如缺少宿主机路径挂载导致 compose 文件不可读）。
- 安全：
  - 自动发现不改变任何 API 鉴权逻辑（无 forward header 仍应返回 `401`）。
  - 最小化 docker-socket-proxy 权限：至少允许读取容器与 labels（`/containers`）；若使用事件驱动可选 `/events`。
  - 不新增对外端口暴露（仍应通过 Traefik/Authelia 等入口访问）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `POST /api/discovery/scan` | HTTP API | external | New | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | UI “立即发现/重新同步” |
| `GET /api/discovery/projects` | HTTP API | external | New | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | “无 Stack 的项目”聚合组数据源 |
| `POST /api/discovery/projects/{project}/archive` | HTTP API | external | New | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | 归档 discovered project |
| `POST /api/discovery/projects/{project}/restore` | HTTP API | external | New | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | 恢复 discovered project |
| `POST /api/stacks/{stack_id}/archive` | HTTP API | external | New | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | 归档 stack（主视图隐藏） |
| `POST /api/stacks/{stack_id}/restore` | HTTP API | external | New | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | 恢复 stack |
| `POST /api/services/{service_id}/archive` | HTTP API | external | New | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | 归档 service |
| `POST /api/services/{service_id}/restore` | HTTP API | external | New | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | 恢复 service（UI 需提供恢复入口） |
| `POST /api/stacks` | HTTP API | external | Delete | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | 移除手动注册 |
| `GET /api/stacks` | HTTP API | external | Modify | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | 增加 archived filter（query） |
| `GET /api/stacks/{stack_id}` | HTTP API | external | Modify | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | service archived 状态用于恢复 |
| Discovery runtime config (env) | Config | internal | New | [./contracts/config.md](./contracts/config.md) | operator | dockrev-api | interval / limits |
| Stack discovery metadata persistence | DB | internal | Modify | [./contracts/db.md](./contracts/db.md) | backend | dockrev-api | 存储 project、lastSeen、冲突/失败原因 |

### 契约文档（按 Kind 拆分）

- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/config.md](./contracts/config.md)
- [contracts/db.md](./contracts/db.md)

## 约束与风险（Constraints & Risks）

- Docker Compose labels 仅对“由 compose 启动的容器”可靠；其他容器应被忽略。
- `config_files` 的格式在不同版本/实现中可能存在差异（分隔符、空白、重复）；必须做健壮解析并在失败时提供可操作日志。
- compose 文件在容器内可读依赖宿主机路径按“同路径只读挂载”进入 Dockrev；缺失挂载是最常见失败原因，必须在 UI/日志中明确提示。
- 本计划将移除“手动注册 stack”能力，并在迁移时清空历史相关数据：这是破坏性变更，必须在发布说明与日志中可见，且需评估对现有用户的影响（例如历史 jobs/backup 记录是否需要保留）。
- “missing 项进程重启后默认隐藏”可能掩盖真实故障/配置问题（例如容器短暂消失、docker 访问失败、compose 文件挂载缺失）；需要提供可追溯信息（lastError/lastSeen/lastScan）并确保项目再次出现时能恢复展示（见开放问题）。
- 允许归档 active 的 stack/service，会导致“主视图看起来一切正常但实际上被收纳了关键对象”。实现阶段需要确保：归档对象仍会参与 check/update 的计算但不会发送通知，且恢复入口足够明显可用（避免误以为系统无数据）。

## 实现入口点（Repo reconnaissance）

实现阶段预计触及/新增的关键位置（用于后续实现定位；本计划不做代码改动）：

- Backend（Rust）：
  - `crates/dockrev-api/src/api/mod.rs`：新增 discovery 相关 API（scan + projects list + hide）
  - `crates/dockrev-api/src/api/types.rs`：新增 discovery API 的 request/response types
  - `crates/dockrev-api/src/config.rs`：新增发现相关 env 配置
  - `crates/dockrev-api/src/db.rs`：存储发现元数据（schema delta + query）
  - `crates/dockrev-api/src/docker_runner.rs`：新增容器/labels 查询能力（复用既有 docker CLI runner 模式）
  - `crates/dockrev-api/src/main.rs`：spawn 周期性 discovery task（与 backup cleanup 类似）
- Web（React）：
  - `web/src/pages/OverviewPage.tsx` / `web/src/pages/ServicesPage.tsx`：新增“立即发现/重新同步”入口与结果展示；展示 stack 的 discovery 状态（missing/冲突/跳过原因）

## UI 设计（Mockups）

说明：本计划的 SVG mockups 复用与 `docs/plan/0001:dockrev-compose-updater/ui/*.svg` 一致的 DaisyUI（dark）token 与 Dockrev app shell（topbar/sidebar）布局，用于保证与现有 Web 界面设计规范一致。

- Overview（新增 discovered group、归档入口、更新计数提示）：`ui/overview.svg`
- Archived / 归档箱（stack 成组、可恢复）：`ui/archived.svg`
- Stack detail（归档 stack/service 入口与恢复动线）：`ui/stack-detail.svg`

## 验收标准（Acceptance Criteria）

- Given Dockrev 已部署且 docker-socket-proxy 可用，且存在一个由 `docker compose up -d` 启动的项目
  When Dockrev 启动并运行自动发现
  Then 60 秒内 `GET /api/stacks` 中出现该项目对应的 stack，且不需要手工注册

- Given 新启动一个 compose 项目（产生带 compose labels 的容器）
  When 下一轮 discovery 扫描到来
  Then 在一个扫描周期内自动注册为新的 stack

- Given 发现到的 `config_files` 在容器内不可读（例如缺少宿主机路径挂载）
  When discovery 扫描尝试注册/同步
  Then 不创建/不更新 stack
  And 返回/记录明确原因（包含“需要把宿主机路径以只读同路径挂载进容器”的可操作提示）
  And 该项目在 UI 的“无 Stack 项目聚合组”中可见（含 reason）

- Given 某个项目曾被发现并注册为 stack，但当前无任何容器存在
  When discovery 扫描运行
  Then 标记为 `missing`
  And 在进程未重启前 UI 可见且有 `missing` 标记
  And 进程重启后默认不在 UI 主视图展示（仍可通过日志/接口排障）

- Given 某个 discovered project 处于 `missing/invalid`
  When 用户在 UI 执行“归档”（调用 `POST /api/discovery/projects/{project}/archive`）
  Then 该项目在 `GET /api/discovery/projects` 中返回 `archived=true`
  And UI 主视图默认不再展示该项目

- Given 某个 discovered project 当前为 `archived=true`
  When 后续扫描再次观察到该 project 的容器（`status=active`）
  Then 服务端自动将 `archived=false` 并恢复在 UI 聚合组中展示

- Given 某个 stack 或 service 被归档（`archived=true`）
  When 用户在 UI 的“已归档”视图中点击恢复
  Then 调用对应 `.../restore` API 成功
  And 该 stack/service 重新出现在主视图

- Given 某个 active stack 或 active service
  When 用户在 UI 执行“归档”
  Then 该对象从主视图隐藏
  And 在“已归档”视图中可见且可恢复

- Given 某个 stack 被归档
  When 用户打开“已归档”视图
  Then 该 stack 以“成组”的形式展示（stack 行 + 其 services 数量/更新数量）
  And 用户可以一键恢复该 stack

- Given 某个 service 已被归档，且其镜像存在可更新版本
  When 系统完成一次 check
  Then UI 中的“有更新的服务数量”应包含该 service
  And 不应发送任何通知（webhook/telegram/email/webpush 等）

- Given 未携带 forward header 请求 `GET /api/stacks`
  When Dockrev 配置 `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`
  Then 返回 `401`

- Given 通过域名入口访问 Dockrev（例如 Traefik/Authelia）
  When 进行 discovery 功能验证
  Then Dockrev 不直接对宿主机暴露额外端口（部署配置保持不变）

- Given 已升级到包含本计划实现的版本（含 DB migration）
  When Dockrev 启动完成
  Then 不再提供手动注册 stack 的入口（UI 与 `POST /api/stacks`）
  And 迁移按约定清理历史“手动注册”相关数据（避免旧数据与新发现机制混杂）

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests（Rust）：
  - `config_files` 解析与规范化（逗号/换行分隔、trim、去重、绝对路径校验）
  - discovered project 状态机：`active ↔ missing`、`archived` 的自动解除/自动归档
  - compose 文件校验：不可读/不可解析的错误分类与可操作提示
- Integration tests（Rust, 使用现有 runner fake 或最小 docker 场景）：
  - 发现到有效 compose labels → 成功创建 stack（composeFiles 写入）
  - compose 文件不可读 → 不创建 stack，返回可操作 reason
  - restart auto-archive：missing 项在启动阶段被标记 archived

### Quality checks

- Rust：`cargo fmt`、`cargo clippy -D warnings`、`cargo test`
- Web：沿用仓库现有 `lint/typecheck/build` 口径（不新增工具链）

## 文档更新（Docs to Update）

- `README.md`：补充“自动发现”行为说明（依赖 docker-socket-proxy + compose 路径同路径只读挂载）
- 部署示例（若仓库已有 `deploy/` 文档/compose）：补充挂载要求与常见错误排障

## 实现前置条件（Definition of Ready / Preconditions）

- Dockrev 的 Docker 访问方式明确且可用：通过 `DOCKER_HOST=tcp://docker-socket-proxy:2375`（或等价）访问 Engine API。
- Dockrev 容器内能读取 compose 文件的“容器视角绝对路径”（宿主机目录按同路径只读挂载）。
- Forward Header 鉴权策略确认：`DOCKREV_AUTH_FORWARD_HEADER_NAME` 生效，且在目标环境 `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`。
- `project → stack` 映射口径已冻结：以 `com.docker.compose.project` 作为唯一键。

## 开放问题（需要主人回答）

None

## 假设（需主人确认）

- 假设 A1：以 `com.docker.compose.project` 作为“项目唯一标识”，用于自动发现创建 stack 的去重。（已确认）
- 假设 A2：发现到的 `config_files` 经规范化（split/trim/dedupe/sort）后作为“变更判定”的唯一形态。
- 假设 A3：除明确移除 `POST /api/stacks` 外，其余既有 API 维持向后兼容（仅增量新增，不移除/不改名既有字段）。
  - 归档逻辑：stack/service archive/restore API；归档对象不发送通知但仍参与 updates 计数
