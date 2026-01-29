# Dockrev: 自动推测当前版本号（floating tag 反推 + UI/交互对齐）（#zdg25）

## 状态

- Status: 部分完成（3/3）
- Created: 2026-01-29
- Last: 2026-01-29

## 背景 / 问题陈述

- 现状：当 Compose 使用 `latest` 等 floating tag 时，Dockrev UI 无法判断“当前运行版本”与 semver tag 的关系，导致列表显示与状态（可更新/需确认/跨 tag）经常变成“需确认 / tag 关系不确定”。
- 目标：在不改变部署方式（仍可用 tag）的前提下，尽可能基于 **运行中容器的 digest** 与 registry 信息反推一个“可解释的当前版本号”（semver tag），并用它改善 UI 显示与交互提示。
- 不做的代价：用户需要手工去 registry/日志对照 tag→digest，才能确认是否跨版本更新或是否真的有更新。

## 目标 / 非目标

### Goals

- 对 floating tag（如 `latest`）自动推测 `resolvedTag`（semver），用于 UI 展示：`latest ≈ v0.2.9`。
- 推测结果用于“跨 tag/同序列”判断，使状态更符合直觉，减少“需确认”。
- 以 digest 为准消除误报：当候选版本 digest 与当前运行 digest 相同，视为“无更新”。
- 推测机制可解释、可降级：推测失败时不影响现有功能（保持原 UI/逻辑）。

### Non-goals

- 不把系统全面切换为 `repo@sha256:...` 的不可变部署（该策略可另立计划）。
- 不对非 semver tag（如 `stable`、`prod`）做智能推断（除非它恰好是 semver 或明确加入规则）。
- 不承诺在“多容器 / 多 digest 并存”情况下给出唯一推断结果（默认降级）。

## 范围（Scope）

### In scope

- 后端 check 逻辑：
  - 在可获取运行中容器信息时，采集该服务的 runtime digest（与该 image repo 对应的 `RepoDigests`）。
  - 基于 runtime digest + registry manifest（按 host platform 解析）推断 `resolvedTag`（semver）。
  - 持久化推测结果并在 stack/service API 中返回。
- Registry digest 解析：
  - 对 multi-arch index，返回 host platform 对应的“平台 digest”，用于与 runtime digest 对齐比较。
- Web UI：
  - 在 Overview/Services/ServiceDetail 等页面展示 `tag ≈ resolvedTag`。
  - 状态判断在存在 `resolvedTag` 时以其为基准（同序列/跨序列）。

### Out of scope

- 增加/修改鉴权模型与权限范围。
- 改动 Compose 发现（discovery）逻辑。

## 需求（Requirements）

### MUST

- 当满足以下条件时，系统必须输出 `resolvedTag`：
  - `image.tag` 不是 semver（例如 `latest`）；
  - 能唯一确定 runtime digest（单个 digest）；
  - registry 中存在至少一个 semver tag，其 manifest digest（host platform）与 runtime digest 相同。
- `resolvedTag` 的选择规则必须稳定且可解释（见“方案概述”）。
- HTTP API 返回的 `resolvedTag/resolvedTags` 必须是可选字段（向后兼容旧前端/外部使用方）。
- UI 在列表与详情页使用 `resolvedTag` 做展示与“同序列/跨序列”判断（若无则回退到 `image.tag`）。
- 若 `candidateDigest == currentDigest`，则该服务在本次 check 结果中必须视为无更新（不应继续显示候选版本）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Stack list/detail includes `resolvedTag(s)` | HTTP API | external | Modify | ./contracts/http-apis.md | Dockrev | Web UI, external clients | `services[].image` 新增字段 |
| Persist inferred resolved tag | DB | internal | Modify | ./contracts/db.md | Dockrev | Dockrev API | `services` 表新增列 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/db.md](./contracts/db.md)

## 验收标准（Acceptance Criteria）

- Given 某服务使用 `image.tag=latest` 且可确定 runtime digest，
  When 运行一次 check，
  Then API 返回 `image.resolvedTag=<某个 semver tag>`，Web 列表显示为 `latest ≈ <resolvedTag>`。

- Given 某服务 `image.tag=latest` 且推测得到 `resolvedTag=0.2.9`，
  When 候选版本为 `0.3.0`，
  Then “跨 tag/同序列”判断以 `0.2.9` 为 current（而不是 `latest`），UI 不再显示“tag 关系不确定”。

- Given registry 上候选 tag 的 digest 与当前运行 digest 相同，
  When 运行一次 check，
  Then 服务不显示候选版本（视为无更新）。

- Given 运行态无法唯一确定 digest（例如多容器且出现多个 digest），
  When 运行一次 check，
  Then `resolvedTag` 不返回（或为 null），UI 行为回退到当前实现（不误导）。

## 实现前置条件（Definition of Ready / Preconditions）

- `services[].image.digest` 的口径冻结为 **运行中容器 digest**（且仅在可唯一确定单一 digest 时返回；否则降级为不返回/空）。
- `resolvedTag` 规则冻结为：在所有 digest==runtime digest 的 semver tag 中，选择“最高 semver”（稳定版优先于 pre-release；与 registry tag 一致，支持 `v0.2.9`/`0.2.9`）。
- `resolvedTags` 需要暴露给 UI：当存在多个 semver tag 指向同一 digest 时，返回所有匹配 tag（按 semver 从高到低），用于 tooltip/解释。
- Dockrev API 运行环境可访问 Docker（能获取运行态 digest）；无法访问时必须降级为“不推测 resolvedTag(s)”且不误导。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests（后端）：
  - multi-arch index 下的 host platform digest 选择；
  - runtime digest→semver tag 推断（含降级分支：无 digest、多 digest、无匹配 tag）。
- Web：
  - Typecheck/build 必须通过。

### UI / Storybook (if applicable)

- 需要新增/更新对应行展示的 story（若仓库已有覆盖相关组件）。

### Quality checks

- 按仓库既有约定运行 lint/typecheck/build（不引入新工具）。

## 文档更新（Docs to Update）

- None

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones）

- [x] M1: 后端 check 采集 runtime digest + 推断 resolvedTag + DB 持久化
- [x] M2: HTTP API 返回字段 + Web UI 展示与状态/提示逻辑对齐
- [x] M3: 补齐关键单测与前端类型/构建门槛

## 方案概述（Approach, high-level）

- runtime digest 采集：优先从 Docker 获取该 compose service 正在运行容器的 image digest（按 image repo 过滤 `RepoDigests`）。
- registry digest 对齐：对 manifest list / OCI index，选择 host platform 对应的子 manifest digest，再与 runtime digest 对比。
- resolvedTag 推断（semver only）：
  - 若候选版本本身为 semver 且其 digest == runtime digest：优先选它作为 `resolvedTag`（可解释且贴近用户预期）。
  - 否则在有限的 semver tags 集合中（按版本从高到低）找第一个 digest 匹配 runtime digest 的 tag 作为 `resolvedTag`。
  - 若存在多个 semver tags 指向同一 digest：选择“最高 semver”作为 `resolvedTag`；并可选返回 `resolvedTags`（供 UI tooltip 或 debug）。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：Dockrev API 若未挂载 docker.sock（或通过 socket-proxy 访问），将无法获得 runtime digest，只能降级到“registry tag digest”，推断质量下降。
- 风险：多副本服务在滚动更新中短时间可能出现多 digest 并存，推断需要降级避免误导。
- 假设：仅对 semver tag 做推断；非 semver tag（如 `stable`/`prod`）不做智能推断（除非后续另加规则）。

## 变更记录（Change log）

- 2026-01-29: 创建计划并冻结验收/契约；冻结 digest=runtime、resolvedTag=highest semver、resolvedTags=all matches；状态更新为 `待实现`。
- 2026-01-29: 完成实现与最小验证（后端/前端/关键测试）；状态更新为 `部分完成（3/3）`（待合入主干或创建 PR 追踪）。

## 参考（References）

- Repo reconnaissance（key files; to be verified in impl）:
  - `crates/dockrev-api/src/api/mod.rs`（check 主流程与 digest/registry 比对）
  - `crates/dockrev-api/src/registry.rs`（manifest/index digest 解析与缓存）
  - `crates/dockrev-api/src/db.rs`（services 持久化字段）
  - `web/src/api.ts`、`web/src/updateStatus.ts`、`web/src/pages/*`（UI 展示与状态判断）
