# Dockrev：digest-only 镜像引用解析与 Discovery 临时 override 回退（#wpnmt）

## 状态

- Status: 已完成
- Created: 2026-03-21
- Last: 2026-03-21

## 背景 / 问题陈述

- 线上存在服务记录被写成 `repo@sha256:digest` + `image_tag=latest` 的状态；Dockrev 能保存这类记录，但 `registry::ImageRef::parse` 仍只接受 `repo:tag`，导致 webhook / check / runtime 路径再次读取时直接报 `invalid image ref`。
- Discovery 目前只在多 variant 冲突场景下才会处理失效的 Dockrev 自生成 override。若单一 variant 里残留了已经消失的 `/tmp/dockrev-override-*.yml` 或 `self-upgrade.override.yml`，项目仍会被标记为 `invalid`。
- 不修复这两点，会继续出现“GHCR 已更新 latest，但 Dockrev 不发现新版本”的漏检与错误排障结论。

## 目标 / 非目标

### Goals

- 让 Dockrev 统一接受 `repo/name[:tag][@sha256:digest]` 形式的镜像引用。
- 保持现有“同一 `image_tag` 的 digest 变化检测”策略不变，只修复建模与读取不一致。
- 让单 variant discovery 在唯一缺失文件是 Dockrev 自生成 override 时回退到稳定 compose 文件，而不是整体失效。
- 补齐回归测试与用户文档，明确 GHCR webhook 与 discovery fallback 的新口径。

### Non-goals

- 不引入跨 tag / 跨 semver 的候选升级发现。
- 不重写 updater 生成临时 override 的机制。
- 不做任何 101 线上一次性数据修复或 DB backfill。

## 范围（Scope）

### In scope

- `registry::ImageRef::parse` 与所有依赖它的服务检查/预检/runtime repo 匹配调用面。
- Discovery 的 `variants.len()==1` 分支对失效 Dockrev 自生成 override 的 fallback 行为。
- 与本次语义变化直接相关的 API reference / troubleshooting 文档。

### Out of scope

- 变更通知策略、update 策略或 registry 交互协议。
- 非 Dockrev 生成的 unreadable compose / override 文件豁免。

## 需求（Requirements）

### MUST

- `repo@sha256:digest` 与 `repo:tag@sha256:digest` 都必须被视为合法镜像引用。
- digest-only 服务记录在 webhook `check.service`、runtime scan、deploy preflight、service digest-tags 调试接口上都不能再被判为 `invalid image ref`。
- 单 variant discovery 若只缺失 Dockrev 自生成 override，必须回退到其余可读 compose 文件并留下 warning。
- 单 variant discovery 若缺失的是用户管理的 compose / override 文件，仍必须保持 `invalid`。

### SHOULD

- 错误文案从“只接受 `repo/name:tag`”更新为能反映 digest-only/tag+digest 也合法。
- 回归测试应直接复现“digest-only 服务记录 + latest 漂移”的真实故障面，而不是只测纯 parser。

### COULD

- 在 discovery details 中记录被忽略的 Dockrev 临时 override，便于后续排障。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 服务记录为 `ghcr.io/acme/web@sha256:old`、`image_tag=latest` 时，GHCR webhook 命中该 repo 后，Dockrev 仍会正常执行 `check.service`，使用 `image_tag=latest` 查询 registry manifest，并在 digest 变化时写入 `candidate_tag=latest` + 新 digest。
- runtime scan 在构建 repo candidate 时，对 digest-only 服务记录应能继续还原出镜像仓库名。
- deploy preflight 在扫描受管服务镜像时，digest-only 引用不再被归入 invalid image refs。
- single-variant discovery 若 compose 文件列表中既有可读稳定文件，又有不可读的 Dockrev 自生成 override 文件（`/tmp/dockrev-override-*.yml` 或 `self-upgrade.override.yml`），则直接丢弃失效 override 并继续使用其余可读文件。

### Edge cases / errors

- `repo/name`（既无 tag 也无 digest）仍是非法镜像引用。
- `repo/name:@digest`、空 digest、空路径仍是非法镜像引用。
- 若 single-variant 中所有文件都不可读，或任一不可读文件不是 Dockrev 自生成 override，则项目继续标记为 `invalid`。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `POST /api/webhooks/github-packages` 描述口径 | HTTP API doc | external | Modify | None | dockrev-api | docs-site readers | 仅更新文档描述，不改 endpoint shape |
| Discovery fallback 口径 | Operations doc | internal | Modify | None | dockrev-api | operators | 文档补充临时 override fallback 说明 |

### 契约文档（按 Kind 拆分）

None

## 验收标准（Acceptance Criteria）

- Given `registry::ImageRef::parse("ghcr.io/acme/web@sha256:old")`,
  When 解析镜像引用，
  Then 返回 `registry=ghcr.io`、`name=acme/web`，且不报 `invalid image ref`。
- Given 受管服务记录为 `image_ref=ghcr.io/acme/web@sha256:old`、`image_tag=latest`，
  When 收到 `acme/web` 的 `package.published` webhook，
  Then 对应 `check.service` 正常完成并写入 `candidate.tag=latest` 与新 digest，而不是记录 `skip service ... invalid image ref`。
- Given service digest-tags 调试接口面向 digest-only 服务记录，
  When 请求 `/api/services/{id}/digest-tags?digest=...`，
  Then 返回 `200` 与 repo tags，而不是因为镜像引用格式被拒绝。
- Given single-variant compose files 为 `[base.yml, /tmp/dockrev-override-xxx.yml]` 或 `[base.yml, self-upgrade.override.yml]` 且仅 override 缺失，
  When 执行 discovery，
  Then 项目继续可用并回退到 `base.yml`，同时留下 fallback warning。
- Given single-variant 中缺失的是用户维护的 override 文件，
  When 执行 discovery，
  Then 项目仍标记为 `invalid`。

## 实现前置条件（Definition of Ready / Preconditions）

- 目标/非目标与边界已经冻结：不做跨 tag 策略变更。
- 文档更新范围已锁定为 API reference 与 troubleshooting。
- 不涉及 schema migration 或线上 backfill。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `cargo test -p dockrev-api registry::tests`
- Integration tests: `cargo test -p dockrev-api github_packages_webhook`, `cargo test -p dockrev-api service_digest_tags`
- Runtime/discovery tests: `cargo test -p dockrev-api discovery`, `cargo test -p dockrev-api runtime_scan`

### UI / Storybook (if applicable)

- None

### Quality checks

- `cargo fmt --check`
- `bun run docs:build`

## 文档更新（Docs to Update）

- `docs-site/docs/api-reference.md`: 更新 GHCR webhook 行为描述为 service-check 优先 + discovery fallback。
- `docs-site/docs/en/api-reference.md`: 同步英文 API 描述。
- `docs-site/docs/zh/api-reference.md`: 同步中文 API 描述。
- `docs-site/docs/troubleshooting.md`: 说明临时 override fallback 与 webhook 排障口径。
- `docs-site/docs/en/troubleshooting.md`: 同步英文故障排查。
- `docs-site/docs/zh/troubleshooting.md`: 同步中文故障排查。

## 计划资产（Plan assets）

- Directory: `docs/specs/wpnmt-digest-only-image-ref-discovery-fallback/assets/`
- In-plan references: `None`
- PR visual evidence source: no screenshots expected.

## Visual Evidence (PR)

None

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 放宽镜像引用解析并修正相关错误文案。
- [x] M2: 修复 single-variant discovery 对失效 Dockrev 临时 override 的 fallback。
- [x] M3: 补齐 digest-only webhook / digest-tags / discovery / runtime 回归测试并同步文档。

## 方案概述（Approach, high-level）

- 在 `registry` 层统一接受 digest-only/tag+digest，避免在每个业务入口重复打补丁。
- 只对 single-variant discovery 缺失 Dockrev 临时 override 的场景做豁免，其余 unreadable file 继续维持严格失败。
- 用现有 API tests / unit tests 基座直接复现线上故障路径，避免引入新的测试 harness。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若某些内部路径隐式依赖 `ImageRef.reference` 一定是 tag，本次放宽解析可能暴露隐藏假设；通过搜索调用面与回归测试兜底。
- 风险：single-variant fallback 若误把用户文件识别成 Dockrev 自生成 override，会掩盖真实挂载问题；因此匹配条件必须限定在 Dockrev 已知的两种文件形态：临时目录下的 `dockrev-override-*.yml` 与 supervisor 的 `self-upgrade.override.yml`。
- 假设：现有脏服务记录在新版本部署后可由下一轮正常 check/discovery 自然恢复，无需额外 backfill。

## 变更记录（Change log）

- 2026-03-21: 新建规格，冻结 digest-only 镜像引用修复与 single-variant discovery fallback 的行为边界。
- 2026-03-21: 完成解析器、discovery fallback、回归测试与 docs-site 文档同步。

## 参考（References）

- `/Users/ivan/.codex/worktrees/6958/dockrev/crates/dockrev-api/src/registry.rs`
- `/Users/ivan/.codex/worktrees/6958/dockrev/crates/dockrev-api/src/discovery.rs`
- `/Users/ivan/.codex/worktrees/6958/dockrev/docs/specs/z3mw5-ghcr-webhook-service-check/SPEC.md`
