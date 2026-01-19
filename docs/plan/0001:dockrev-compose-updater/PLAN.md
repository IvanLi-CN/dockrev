# Dockrev: Docker/Compose 更新管理器（#0001）

## 状态

- Status: 部分完成（4/7）
- Created: 2026-01-18
- Last: 2026-01-19

## Change log

- 2026-01-19: 完成 M1（后端：SQLite schema + HTTP API scaffold + Compose services 解析）
- 2026-01-19: 完成 M2（registry tag 扫描 + manifest 解析 + arch 匹配提示 + ignore 生效）
- 2026-01-19: 完成 M3（更新执行：compose pull/up + healthcheck 等待 + digest 回滚闭环）
- 2026-01-19: 完成 M4（Web UI：Stacks/Ignores/Jobs/Settings + /api 代理 + 基本交互闭环）

## 背景 / 问题陈述

Dockrev 的目标是为单机 Docker/Compose 环境提供“可控、可审计、可回滚”的容器更新体验：

- 仅基于镜像 tag 变化来提示/触发更新，并对 manifest 的 arch 兼容性给出不同提示。
- 允许按 service / compose stack / 全部 三种范围触发更新，并支持服务粒度忽略规则。
- 更新执行必须遵循 Docker healthcheck，并在失败时按 digest 强制回滚到已知可用版本。
- 通过 Web UI（React + TypeScript）完成配置、审阅、触发与观察；后端用 Rust 实现。

## 目标 / 非目标

### Goals

- 在 Web UI 中展示 stacks/services 的当前镜像（tag + digest）、可用更新（tag + digest）与 arch 匹配结果。
- 支持三种更新范围：
  - 单 service/镜像更新
  - 单 compose stack 更新
  - 全部更新
- 支持“忽略名单”（服务粒度），并可在 UI 中配置、启用/禁用、查看生效范围。
- 支持 webhook 触发更新（同样支持 service/stack/all 范围）。
- 支持健康检查与失败回滚（digest），回滚行为可在 UI 配置（超时、是否自动回滚、回滚后是否告警）。
- 支持通知：Email / Webhook / Telegram / Web Push（Chrome, VAPID）。
- registry 支持：GHCR / Harbor / 自建 registry，凭据只读 `~/.docker/config.json`（volume bind）。
- 鉴权：单用户，Forward Header 模式（反向代理注入，后端只校验并信任来源）。

### Non-goals

- 不做 Kubernetes 支持。
- 不做多租户/多用户权限模型（仅单用户）。
- 不在 MVP 中实现“无限制的自动化更新策略语言”（只做少量可配置策略）。
- 不在 MVP 中实现复杂镜像签名/供应链校验（可在后续计划补充）。

## 范围（Scope）

### In scope

- Docker Engine 连接（`/var/run/docker.sock`）与容器/镜像信息读取。
- registry 查询：tag 列表/manifest（含 manifest list）解析，得到 digest 与 arch 列表。
- host arch 探测（运行 Dockrev 的宿主机/容器环境）并参与匹配提示。
- 更新执行：
  - 记录目标 digest（更新前/更新后）
  - 执行拉取与重启（策略见“方案概述”）
  - 等待 healthcheck 达标
  - 失败时按记录的 digest 回滚并复测
- 持久化：SQLite（运行时 volume）。
- 前端页面：概览、stack 详情、service 详情、忽略规则、通知配置、更新队列/历史与日志。
- Web Push：订阅/退订/发送测试通知。

### Out of scope

- 自动发现所有 compose 文件并自动改写它们（MVP 以“显式注册 stack”为主；自动发现作为后续增强）。
- 自动创建/管理反向代理与 TLS（由部署方负责）。
- 自动解决镜像 tag 与语义版本（semver）以外的“非结构化版本规则”（仅提示，不做强推断）。

## 需求（Requirements）

### MUST

- “新版本提示”仅依据 tag 是否出现更新（不依赖 GitHub release）。
- UI 必须区分并提示：
  - **arch 匹配**：候选 tag 的 manifest 含 host arch（可更新）。
  - **arch 不匹配**：候选 tag 的 manifest 不含 host arch（提示但默认不允许更新，除非用户显式覆盖）。
- 服务列表必须按 docker compose stack 分组显示，并支持分组折叠/展开（用于降低视觉噪音与提升可扫读性）。
- 支持忽略规则（服务粒度）：
  - 可按 tag（精确/前缀/regex）或 semver 范围表达忽略（具体形状见契约）。
  - 忽略后不产生“可更新”提示，但保留可追溯记录。
- 支持 webhook 触发更新（service/stack/all），且更新请求必须可审计（谁触发、参数、结果、日志摘要）。
- 健康检查仅使用 Docker healthcheck：
  - 对无 healthcheck 的容器：默认允许更新，但**不自动回滚**（仍记录 digest 与告警）。
- 更新前支持“临时数据备份”（可在系统设置启用/禁用，默认启用且失败阻止更新）：
  - 备份面向 compose stack，并由 stack 定义“备份目标”（Docker named volumes + bind mounts，见契约）
  - 当启用备份时，更新任务在执行更新前先执行一次备份，并将备份结果写入审计
  - 备份前必须计算每个 target 的体积：
    - 对服务未表态（? / inherit）的目标：超过阈值则跳过（默认阈值 `100MiB`，可在系统设置配置）
    - 对服务显式勾选（☑ / force）的目标：不受该阈值限制
  - 服务可对每个备份目标做三态选择（服务粒度）：
    - ☑（force）：强制备份
    - ☐（skip）：强制不备份
    - ?（inherit）：未决定，按系统级默认策略（包含阈值跳过）
- 回滚策略（服务粒度可配置）：
  - 每个服务可配置是否自动回滚（默认：有 healthcheck 的服务启用；无 healthcheck 的服务禁用）
  - 自动回滚时必须以 digest 为基准，提供“可复现的回滚”（记录 old/new digests）
- 通知支持：Email / Webhook / Telegram / Web Push（Chrome, VAPID）。
- 凭据：仅从 `~/.docker/config.json` 读取（不会在 UI 中展示明文 token）。
- 鉴权：默认要求 Forward Header `X-Forwarded-User`；本地开发模式允许跳过；header 名称允许用环境变量覆盖（见契约）。

### SHOULD

- 支持“同级别提示”：当当前 tag 为 `5.2`，出现 `5.3` 时提示为“同级别更新候选”，允许被忽略或配置提示策略。
- 支持“检查与更新分离”：
  - webhook 可只触发“检查”（刷新候选版本）
  - 或触发“执行更新”
- 支持并发控制：
  - 同一 stack 在同一时刻只允许一个更新任务
  - 不同 stack 可并行（可配置并发上限）
- 提供可导出的审计记录（JSON 导出或 DB 备份说明）。
- 备份保留策略（默认）：
  - 每个 stack 仅保留 1 份“最近备份”
  - 当一次更新成功且稳定运行 1 小时后，自动删除对应备份（避免长期占用磁盘）

### COULD

- 支持计划任务（cron-like）定期检查与自动更新（在 UI 明确开启且默认“保守策略”）。
- 支持 registry 事件 webhook（Harbor/GHCR）自动触发“检查”。
- 支持更新“演练模式”（dry-run：仅计算计划，不执行更新）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Dockrev HTTP API | HTTP API | external | New | ./contracts/http-apis.md | backend | web | 供 UI 调用，含 webhook、推送订阅、更新任务 |
| Update events | Event | internal | New | ./contracts/events.md | backend | backend | 用于队列/状态机与通知触发 |
| SQLite schema | DB | internal | New | ./contracts/db.md | backend | backend | 记录 stacks/services、候选版本、任务与审计 |
| Compose stack definition | File format | internal | New | ./contracts/file-formats.md | backend | backend/web | stack 注册、忽略规则与 compose 路径形状 |
| Docker Compose runner | CLI | internal | New | ./contracts/cli.md | backend | backend | 以 `docker compose` 为主的更新执行与状态探测 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/events.md](./contracts/events.md)
- [contracts/db.md](./contracts/db.md)
- [contracts/file-formats.md](./contracts/file-formats.md)
- [contracts/cli.md](./contracts/cli.md)

## 验收标准（Acceptance Criteria）

- Given 已注册一个 stack，且其 service 使用镜像 `repo/app:5.2`
  When registry 出现 `repo/app:5.3` 且 manifest 含 host arch
  Then UI 显示“可更新”与候选版本 `5.3`（含 digest），并允许执行更新

- Given registry 出现 `repo/app:5.3` 但 manifest 不含 host arch
  When UI 刷新候选版本
  Then UI 显示“arch 不匹配”提示，并默认不允许执行更新（除非用户显式覆盖）

- Given 用户配置 service 忽略规则（例如忽略 `5.3.*` 或某个 semver 范围）
  When registry 出现满足忽略条件的 tag
  Then UI 不应将其计入“可更新”，但应可在详情中看到“已被忽略”的记录与原因

- Given webhook 触发“更新某个 service”
  When 后端接收请求且鉴权通过
  Then 创建一条更新任务，任务在 UI 中可见，并包含触发来源与参数

- Given 触发一次更新且更新后容器 healthcheck 在超时前变为 healthy
  When 任务完成
  Then 任务状态为 success，且记录 new digest 与耗时，并发送通知（按配置）

- Given 触发一次更新且更新后 healthcheck 超时或变为 unhealthy
  When 任务失败
  Then 后端按 old digest 回滚并再次等待 healthcheck
  And UI 中任务状态为 rolled-back（或等价），并发送失败/回滚通知

（备份相关）

- Given 系统设置启用“更新前备份”，且 stack 配置了备份目标
  When 触发一次更新
  Then 更新任务必须先执行备份，并在 UI 显示备份产物路径与体积（或失败原因）

- Given 系统设置阈值为 `100MiB`
  When 备份目标体积超过阈值
  Then 该目标被跳过并在任务日志中标注“skipped_by_size”

- Given 系统设置要求备份成功（fail-closed）
  When 备份步骤失败（非跳过）
  Then 更新任务不应继续执行更新，并报告失败原因与通知

- Given 更新成功且相关服务在 1 小时内持续 healthy
  When 达到 1 小时稳定窗口
  Then 自动删除该次更新关联的备份产物，并写入审计记录

（补充关键边界与异常见“测试与质量门槛”）

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests（Rust）：
  - semver/忽略规则匹配与“同级别提示”判定
  - registry manifest 解析（含 manifest list）与 arch 匹配逻辑
  - 更新状态机（happy path / rollback path）
- Integration tests（Rust, 可在 CI 中用最小 Docker 场景模拟）：
  - 以带 healthcheck 的测试容器验证“更新→等待→回滚”
  - 以 multi-arch manifest fixture 验证 arch 分支提示
  - 备份：模拟 docker volume + bind mount 的备份、体积阈值跳过、fail-closed 行为与清理任务
- E2E（Web）：
  - MVP 仅覆盖关键页面可渲染与基本交互（不强制引入新测试框架；后续再补齐）

### UI / Storybook (if applicable)

- 本仓库当前未引入 Storybook；本计划不引入新工具。
- UI 视觉规范参考 DaisyUI（flat, `--depth: 0`, `--noise: 0`），并维护可落地的主题草案：`docs/plan/0001:dockrev-compose-updater/ui/daisyui-theme.md`。
- UI 设计图（深色/亮色）与可视化预览：
  - 预览页：`docs/plan/0001:dockrev-compose-updater/ui/preview.html`（含 `Measure` 按钮用于浏览器内测量行/卡片内边距）
  - 设计图：`docs/plan/0001:dockrev-compose-updater/ui/dashboard*.svg`、`docs/plan/0001:dockrev-compose-updater/ui/service-detail*.svg`、`docs/plan/0001:dockrev-compose-updater/ui/system-settings*.svg`

### Quality checks

- Rust：`cargo fmt`、`cargo clippy -D warnings`、`cargo test`
- Web：`npm run lint`、`tsc`、`vite build`

## 文档更新（Docs to Update）

- `README.md`: 同步“运行方式/部署方式/端口/鉴权模式”与“凭据读取方式”
- `docs/plan/README.md`: 计划索引与状态更新（本次已创建）

## 里程碑（Milestones）

- [x] M1: 数据模型与契约冻结（contracts + db schema + API 列表）
- [x] M2: registry 扫描与 arch 匹配提示可用（不含更新执行）
- [x] M3: 更新执行 + healthcheck + digest 回滚闭环
- [x] M4: Web UI（概览/详情/忽略规则/任务与日志）
- [ ] M5: 备份（volume+bind mounts）与保留/清理策略
- [ ] M6: 通知与 Web Push（含订阅管理与测试发送）
- [ ] M7: 部署文档与“最小生产”运行手册

## 方案概述（Approach, high-level）

- “检查（detect）”与“执行（apply）”拆分：
  - 检查阶段负责：发现候选 tag、解析 digest、评估 arch、计算忽略规则、生成更新计划
  - 执行阶段负责：按计划更新、等待 healthcheck、失败回滚、写审计记录、发送通知
- “显式注册 stack”为 MVP 主路径：
  - 由用户在 UI 中提供 compose stack 的路径/标识与 service 映射关系
  - 后续再增强自动发现（基于 Docker labels 或配置扫描目录）
- digest 是回滚与审计的唯一基准；UI 里必须能看到“更新前/更新后”的 digest。
- 更新执行以 `docker compose` 为主：
- 更新执行以 Compose CLI 为主（默认 `docker-compose`，可配置切换到 `docker compose`）：
  - stack 注册提供 `composeFiles` / `envFile`（容器内绝对路径）
  - 执行阶段通过 compose CLI 进行 `pull` / `up -d`（按 service/stack/all 范围）
  - healthcheck 状态通过 Docker Engine 读取（compose 负责变更，engine 负责观测）
- “备份”作为更新前置步骤（可配置）：
  - 启用时：先生成 stack 级备份，再执行更新
  - 默认 fail-closed：备份失败则阻止更新继续执行
  - 备份目标包含 docker volumes 与 bind mounts；服务可对每个目标做三态选择（UI：☑/☐/?）：
    - ☑（force）：强制备份
    - ☐（skip）：强制不备份
    - ?（inherit）：服务未表态，按系统级默认策略执行
  - 备份前执行体积检测：
    - 仅对 `inherit` 的目标应用“超过阈值则跳过并记录”（默认 100MiB）
    - `force` 的目标不受该阈值限制（仍需记录体积与耗时，避免误操作）
  - 备份产物默认保留 1 份；更新成功稳定 1 小时后自动删除

## 风险与开放问题（Risks & Open Questions）

- 风险：
  - 不同 registry 的 tag/manifest API 细节与限流差异导致扫描不稳定（需要缓存与退避）。
  - Compose 栈的“文件路径/服务映射”在不同部署方式下差异大（需要明确可配置形状）。
  - 无 healthcheck 的容器如何判定成功/失败（必须有默认策略与 UI 提示）。

- 需要决策的问题：
  - bind mounts 的体积检测与备份性能：默认采用 `du` 扫描可能较慢，是否需要可配置的超时/采样策略？
  - “稳定运行 1 小时”的判定：
    - 对有 healthcheck：持续 healthy
    - 对无 healthcheck：用“持续 running 且 1 小时内未重启/退出”替代
  - “同级别提示”的精确定义：仅 semver minor 变化，还是允许更通用的“数值段提升”。

## 假设（需主人确认）

- 默认通过反向代理注入 `X-Forwarded-User`（或等价 header），Dockrev 仅在该 header 存在时放行（开发模式可放宽）。
- Dockrev 运行容器可只读挂载 `~/.docker/config.json` 与读写挂载 SQLite 数据目录。
- Dockrev 运行容器可读写挂载 `DOCKREV_BACKUP_BASE_DIR`（默认 `/data/backups`）用于备份产物。

## 参考（References）

- Docker/OCI Distribution API（tag/manifest/digest 概念）
