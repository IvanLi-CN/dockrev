# Dockrev: 修复 supervisor 自动识别（labels-first + inspect fallback）+ 更新时 best-effort 拉 semver tag（避免悬空）

## 状态

- Status: 待实现
- Created: 2026-02-21
- Last: 2026-02-21

## Change log

- 2026-02-21: 创建计划，冻结目标与验收标准。

## 背景 / 问题陈述

Dockrev 的自我升级（`/supervisor`）需要在目标宿主机上找到“正在运行的 dockrev 容器”，以便读取 compose labels 并执行 `docker compose up`。

当前 supervisor 的自动识别逻辑主要依赖：

- `docker ps` 输出中的 `Image` 字符串是否能匹配 `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO`（repo / `repo:tag` / `repo@digest`）。

但在生产环境常见场景中会失败：

- 运行中的 dockrev 容器仍在使用旧镜像（例如 `0.7.5`），与此同时远端 `:latest` 已指向新镜像（例如 `0.7.7`）；
- 旧镜像因此变为 dangling（没有 `RepoTags/RepoDigests`），导致 `docker ps` 的 `Image` 字段显示为短 image id（例如 `c85819d0c6dd`）；
- supervisor 误判“没有匹配容器”，要求人工配置 `DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID`，从而让自我升级卡死。

同时，dangling 也会降低可观测性（运维看到的镜像来源不再是明确的 repo:tag），并增加后续自动识别失败概率。

## 目标（必须满足）

### A) supervisor 自动识别（必须稳定）

1. 当配置了 `DOCKREV_SUPERVISOR_TARGET_COMPOSE_PROJECT`/`...SERVICE` 时，auto-match 必须 **labels-first**，通过 compose labels 直接锁定目标容器，即使 `docker ps` 显示短 image id 也能成功。
2. 未配置 compose project/service 时，保留现有 repo 匹配行为；若 repo 匹配失败，必须走 **inspect fallback**（通过 `docker inspect` 的 `Config.Image` 做 repo 匹配）覆盖 dangling 场景。
3. 多候选时必须保持保守（消歧失败则要求显式配置 `DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID`）。

### B) semver tag 拉取（best-effort，避免悬空）

1. 覆盖范围：supervisor 自我升级流程 + dockrev 普通更新任务（更新任意服务）。
2. 拉取对象：只拉“目标版本”的 semver tag（例如 `0.7.7`）。
3. 失败策略：best-effort（记录 warning，但不阻断更新）。

## 非目标（明确不做）

- 不引入 strict 模式（pull semver 失败不算升级失败）。
- 不要求 registry 一定存在 semver tag（可能没有或无权限）。
- 不改变“更新锁定 digest”的主语义：更新仍以 digest pin 为准。

## 行为与接口（冻结）

### semver tag 推断来源（固定）

- 从镜像 OCI label：`org.opencontainers.image.version` 读取。
- 解析规则：
  - `trim()`
  - 允许前缀 `v`/`V`（例如 `v0.7.7` → `0.7.7`）
  - 使用 `semver` 解析；若包含 build metadata（`+...`）则跳过（Docker tag 不允许 `+`）

### semver pull 时机（固定）

- supervisor：pull 目标镜像后，best-effort pull `repo:<semver>`。
- dockrev 普通更新：对每个实际执行了更新的 service，在拿到最终 `new_image_id` 后，best-effort pull `repo:<semver>`；同一次 job 里对相同 `repo:<semver>` 去重。

## 验收标准（Given / When / Then）

1. Given 目标容器的 `docker ps` 中 `Image` 为短 id（dangling），When supervisor resolve target，Then 仍能识别到目标容器并继续流程（不再报 “no running container matched …”）。
2. Given 设置了 `DOCKREV_SUPERVISOR_TARGET_COMPOSE_PROJECT/SERVICE`，When resolve target，Then 优先按 compose labels 命中目标容器。
3. Given 目标镜像带 `org.opencontainers.image.version=0.7.7` 且 registry 存在 `repo:0.7.7`，When 更新完成，Then best-effort pull 后本地 `RepoTags` 包含该 semver tag。
4. Given semver tag 不存在/无权限/限流，When 更新执行，Then 更新不失败，但日志/summary 中存在 warning。

## 测试计划（必须做）

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --locked --all-features`
- 单测必须覆盖 dangling 场景（fake docker：`docker ps` 输出 image id；`docker inspect` 的 `Config.Image` 为 `repo:tag`）。

## 里程碑（Milestones）

1. supervisor：auto-match 改为 labels-first + inspect fallback；补齐回归测试。
2. 更新任务：best-effort semver pull（supervisor + dockrev-api）与回归测试；summary/log 输出清晰。
3. 文档合同同步：更新 Plan 0012 的 config contract，说明新的 resolution order 与 semver pull 行为。

