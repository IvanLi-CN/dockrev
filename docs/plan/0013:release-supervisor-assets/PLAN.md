# CI/CD: GitHub Release 追加发布 dockrev-supervisor 二进制包（#0013）

## 状态

- Status: 待实现
- Created: 2026-01-26
- Last: 2026-01-26

## 背景 / 问题陈述

- 当前 `dockrev-supervisor` 仅随 GHCR 单镜像 `ghcr.io/ivanli-cn/dockrev:<semver>` 一起发布（镜像内包含 `/usr/local/bin/dockrev-supervisor`），GitHub Release Assets 仅包含 `dockrev` 的 Linux 二进制包（amd64/arm64 × gnu/musl）。
- 当用户不使用容器部署（或希望在宿主机直接运行 supervisor）时，无法从 GitHub Release 直接获取 supervisor 可执行文件；同时“supervisor 与 dockrev 的版本/tag 对齐”口径也不够显式可验证。

## 目标 / 非目标

### Goals

- 在 GitHub Release Assets 中额外发布 `dockrev-supervisor_*` tarball（与 `dockrev_*` 并列），并提供对应 checksum 文件（`*.tar.gz.sha256`）。
- 保持现有 `dockrev_*` Release assets 行为不变（文件名规则、产物矩阵、checksum 生成规则）。
- 明确对外交付物口径：Release assets 覆盖 `dockrev` 与 `dockrev-supervisor` 两个二进制。

### Non-goals

- 不拆分为两个独立镜像或独立仓库。
- 不新增 Windows/macOS 发行包。
- 不改变现有 GHCR 镜像的“单镜像+双可执行文件”策略（本计划只增加 Release assets 维度的交付）。

## 范围（Scope）

### In scope

- 调整 `.github/workflows/release.yml` 的“打包 Release assets”逻辑：在现有 `dockrev_*` 之外，新增 `dockrev-supervisor_*` 的打包与 checksum（`*.tar.gz.sha256`）生成，并一并上传到 GitHub Release。
- 文档口径更新：明确 Release assets 的组成（`dockrev` + `dockrev-supervisor`）。

### Out of scope

- 更改 `dockrev-supervisor` 的运行方式、配置、路由（例如 `/supervisor/*` 反代策略）。
- 更改版本号策略（仍以 PR intent label + semver bump 为准）。

## 需求（Requirements）

### MUST

- GitHub Release Assets 新增以下文件（示例以 `0.3.0` 为例）：
  - `dockrev-supervisor_0.3.0_linux_amd64_gnu.tar.gz` + `dockrev-supervisor_0.3.0_linux_amd64_gnu.tar.gz.sha256`
  - `dockrev-supervisor_0.3.0_linux_arm64_gnu.tar.gz` + `dockrev-supervisor_0.3.0_linux_arm64_gnu.tar.gz.sha256`
  - `dockrev-supervisor_0.3.0_linux_amd64_musl.tar.gz` + `dockrev-supervisor_0.3.0_linux_amd64_musl.tar.gz.sha256`
  - `dockrev-supervisor_0.3.0_linux_arm64_musl.tar.gz` + `dockrev-supervisor_0.3.0_linux_arm64_musl.tar.gz.sha256`
- 每个 `dockrev-supervisor_*.tar.gz` 解压后包含一个顶层可执行文件：`dockrev-supervisor`。
- `*.tar.gz.sha256` 文件的生成规则与现有 `dockrev_*` 保持一致（同一工具与输出格式）。
- 仍然发布并保持现有 `dockrev_*` Release assets 不变（数量、命名、内容）。
- 失败行为清晰：
  - 若 GHCR 镜像 push 成功但 Release upload 失败：工作流按现有策略失败并给出明确重试指引（不引入“自动回滚镜像”）。
- 发布矩阵与 `dockrev` 一致：
  - OS: `linux`
  - arch/libc: `amd64/arm64 × gnu/musl`

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| GitHub Release assets: `dockrev-supervisor_<ver>_linux_<arch>_<libc>.tar.gz` + `.tar.gz.sha256` | File format | external | New | ./contracts/file-formats.md | CI/CD | end users / deploy scripts | 与 `dockrev_*` 并列发布 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/file-formats.md](./contracts/file-formats.md)

## 验收标准（Acceptance Criteria）

- Given 触发一次有效发版（release intent label 为 `type:patch|minor|major` 且 Release workflow 成功）
  When 打开对应 tag 的 GitHub Release 页面
  Then 能看到 `dockrev_*`（原有 8 个 assets）与 `dockrev-supervisor_*`（新增 8 个 assets）并列存在。
- Given 下载任一 `dockrev-supervisor_*.tar.gz` 与其 `*.tar.gz.sha256`
  When 校验 sha256 并解压
  Then 校验通过，且解压后得到 `dockrev-supervisor` 可执行文件。
- Given 同一次发版
  When 拉取 `ghcr.io/ivanli-cn/dockrev:<semver>`
  Then 镜像中仍包含 `/usr/local/bin/dockrev` 与 `/usr/local/bin/dockrev-supervisor`（不回退现有交付方式）。

## 实现前置条件（Definition of Ready / Preconditions）

- 对外发布矩阵已确认（arch/libc 的组合与 `dockrev_*` 保持一致，且不会新增其它平台）。
- 产物命名与内容契约已确认并记录（见 `contracts/file-formats.md`）。
- 文档更新范围已确认（最少包含 `README.md` 的 “Releases / Images” 口径更新）。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- CI (main): 不新增额外测试类型；以现有 checks 为门槛。
- Release workflow: 增加“打包后自校验”步骤（例如：逐个解包并校验文件存在 + sha256 校验），确保在 upload 前失败可见。

### Quality checks

- `.github/workflows/release.yml` 变更需保持幂等、可重试（Re-run jobs 可重复产出一致 assets）。

## 文档更新（Docs to Update）

- `README.md`: “GitHub Releases include Linux binaries …” 口径更新为包含 `dockrev` + `dockrev-supervisor`。

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones）

- [ ] M1: 在 `release.yml` 中打包并上传 `dockrev-supervisor_*`（含 `*.tar.gz.sha256`）
- [ ] M2: 为新增 assets 增加 workflow 内自校验（打包后校验 + sha256 校验）
- [ ] M3: 更新 `README.md` 的发布物说明并与实现对齐

## 方案概述（Approach, high-level）

- 复用现有 `Package release assets` 逻辑：为 `dockrev-supervisor` 追加一个与 `dockrev` 对称的打包函数或参数化打包函数，输出到同一 `dist/release/` 目录，交给 `release-action` 统一上传。
- 契约先行：对新增文件名规则、tar 内容布局、checksum 输出格式在 `contracts/file-formats.md` 冻结后再实现。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：Release assets 数量翻倍（体积与上传耗时增加），可能增加 Release job 的失败概率；需通过自校验与清晰重试指引控制风险。
- 需要决策的问题：None
- 已确认：`dockrev-supervisor` 的版本与 `dockrev` 完全一致（同一 tag），无需独立版本线。

## 变更记录（Change log）

- 2026-01-26: 创建计划。

## 参考（References）

- `.github/workflows/release.yml`
- `Dockerfile`（镜像内包含 `dockrev-supervisor`）
