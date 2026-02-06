# Dockrev: Self-upgrade 后不应触发 `config_files_conflict`（归一 + warning）

## 背景 / 问题陈述

Dockrev 的 auto-discovery 依赖 Docker Compose 自动写入的 label：

- `com.docker.compose.project`
- `com.docker.compose.project.config_files`（绝对路径列表，顺序有语义）

当 `dockrev-supervisor` 执行 Dockrev 自我升级时，会在 `docker compose up` 额外追加一个 override compose file（例如 `self-upgrade.override.yml`），导致 **同一 compose project 内不同容器的 `config_files` 集合不一致**。Dockrev 目前将其直接判为 `invalid/config_files_conflict`，进而导致：

- 该 stack 在 UI 被标为 invalid（可用性/可信度下降）
- 服务列表/镜像信息可能与运行态不一致（至少在一段时间内误导运维判断）

## 目标 / 非目标

### Goals

- MUST：执行一次 Dockrev 自升级后，目标 compose project 仍保持 `active`（不因 `config_files_conflict` 变 `invalid`）。
- MUST：Dockrev 以可解释、确定性的规则选择稳定且可用的 stack compose files（即使存在 override）。
- MUST：UI 服务列表中 `dockrev-supervisor` 的镜像 repo/tag 信息与运行态一致（至少在一次 scan 后一致）。
- SHOULD：对“因自升级引入 override”这种冲突给出明确提示与可操作建议（warning，而不是简单 invalid）。
- COULD：减少对“额外 override 文件路径必须在 Dockrev 容器内可读”的运维要求（降低踩坑概率）。

### Non-goals

- 不重做 discovery/stack 模型（仅改 config_files 归一与冲突诊断）。
- 不支持非 Compose 的编排系统。
- 不强制写回用户的 compose 文件（仍采用现有 override 策略）。

## 范围（In / Out）

### In scope

- discovery 收集/归一同 project 内的 `config_files` variants，并选择 canonical compose files。
- 针对 “superset（多一个 override）” 的场景降级为 warning（而非 invalid）。
- 提供冲突诊断信息（variants → services 映射）。
- UI 展示 warning（`active` + `lastError`）并避免误导。
- 单元测试与最小集成测试覆盖。
- README / deploy 文档补充说明与推荐配置。

### Out of scope

- 引入新的 discovery 状态枚举（不新增 `degraded`；复用 `active + warning`）。
- 变更 updater 的 guardrails / rollback 策略。

## 方案（冻结）

### Canonical 选择（确定性）

对同一 `com.docker.compose.project`，若存在一个 `config_files` 列表是其余所有列表的 **超集（按顺序 subsequence）**，则选择该超集为 canonical，并记录 warning。

### Override 安全校验（必须）

仅当超集中的 extra compose files 满足 “image-only override” 且只覆盖使用该 variant 的 service 时，才允许自动消解为 warning；否则仍判 invalid，并输出诊断信息。

### Extra 不可读时回退（必须）

若 canonical 超集中的 extra compose file 在 Dockrev 容器内不可读（常见于临时/未挂载路径），则回退到所有容器共有的 compose files（交集）作为 canonical，project 仍为 `active`，并记录 warning + 运维建议。

## 验收标准（DoD）

- Given：同一 project 内存在两种 config_files 列表，其中一种是另一种的超集（多一个 override 文件）
  - When：执行 discovery scan
  - Then：该 project 不标记为 invalid；canonical 选择超集；并记录 warning（说明存在 override）
- Given：config_files 中存在重复路径（同一路径出现多次）
  - When：解析并归一
  - Then：去重后仍能正确工作，不触发误判
- Given：存在多个 distinct config_files 列表，且互相都不是子集关系
  - When：scan
  - Then：仍可标记 invalid，但必须提供可诊断信息（列出各列表、来自哪些 service）
- Given：superset 的 extra compose file 在 dockrev 容器内不可读
  - When：scan
  - Then：回退到交集作为 canonical；project 仍 active；warning 提示如何挂载/配置
- 回归：无 override 场景的 discovery/scan 行为不变

## Testing

- Unit tests（Rust）：
  - `config_files` variants 归一：subsequence/superset 选择、去重、非子集冲突诊断
  - “image-only override” 校验（安全边界）
  - extra 不可读时回退到交集
- Minimal integration（Rust）：
  - 用 fake runner 模拟 docker inspect 两个 service 的 labels（base vs base+override）
  - 断言 scan 结果为 active + warning，并产生稳定 canonical

## 风险

- 风险：canonical 选择错误会导致更新时使用错误的 compose files，引入覆盖/回滚问题。
  - 缓解：仅对 “image-only override” 的 extra files 自动消解；否则保持 invalid 并输出诊断。

