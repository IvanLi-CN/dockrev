# CI/CD: GitHub Actions 构建提速（#0005）

## 状态

- Status: 待实现
- Created: 2026-01-21
- Last: 2026-01-21

## 背景 / 问题陈述

- 仓库当前存在三条关键 GitHub Actions workflows：
  - `CI (PR)`：`.github/workflows/ci-pr.yml`
  - `CI (main)`：`.github/workflows/ci-main.yml`
  - `Release`：`.github/workflows/release.yml`
- baseline 观测（Release run）：
  - Run: `IvanLi-CN/dockrev` → Actions run `21197273148`（`Release`，2026-01-21）
  - `Build release binaries (linux/amd64)`：约 5 分钟（`2026-01-21T04:30:36Z` → `04:35:35Z`）
  - `Build release binaries (linux/arm64)`：截至 `2026-01-21T05:10:09Z` 仍在运行（已 > 34 分钟），显著慢于 amd64
- baseline 观测（CI runs）：
  - `CI (PR)` run `21196665469`（2026-01-21，总耗时约 10 分钟）：
    - `Lint & Checks`：约 2 分钟
    - `Backend Tests`：< 1 分钟
    - `Release build check (PR)`：约 8 分钟（主要耗时在 `Build dockrev (linux/amd64 musl)` ~3 分钟 + `Docker build` ~4 分钟）
  - `CI (main)` run `21197239891`（2026-01-21）：
    - `Lint & Checks`：约 2 分钟
    - `Backend Tests`：< 1 分钟
- 现状（从 workflows 静态勘察得到的“可能耗时点/重复工作”）：
  - `CI (PR)` / `CI (main)` 的多个 job 都会执行 `Set up Node.js` + `npm ci` + `npm run build`（web build）。
  - `lint` job 还会执行 `storybook build` + `playwright install --with-deps` + `test-storybook`（通常较重）。
  - `pr-release-check` job 会执行 Rust musl release build 与 smoke test，随后再执行 Docker build；而 `Dockerfile` 本身也会再次进行 web build 与 Rust musl release build，存在重复构建的风险。
- 关键事实（可用于“减少不必要工作”）：
  - `crates/dockrev-api/build.rs` 在 `web/dist` 不存在时会生成 placeholder `index.html`，因此 Rust build/test **不强依赖** web build 成功（除非我们刻意把 web build 作为 CI gate）。

## 已确认决策（主人已拍板）

- 目标范围：同时优化 `CI (PR)`、`CI (main)`、`Release` 三条链路。
- arm64 构建策略：使用“原生 arm64 runner”构建 arm64 产物，避免 QEMU 下的 `docker run --platform linux/arm64 ...` 编译瓶颈。
- Docker build cache：可以启用（Buildx + GitHub Actions cache backend）。
- cache 写入失败语义：允许继续（best-effort cache；缓存写入失败不应让 build 失败）。
- PR gating：允许按变更路径跳过重型 job（前端重型检查、`Release build check (PR)`）。
- Docker 构建方式：采用“产物优先”（不在 `Dockerfile` 内编译 Rust/web；由 workflow 先产出二进制与 `web/dist`，docker build 只做打包）。
- 原生 arm64 runner：优先使用 `ubuntu-24.04-arm`（保留 `ubuntu-22.04-arm` 作为 fallback）。
- 时间目标（冷 cache；作为验收基线）：
  - `CI (PR)`：≤ 6 分钟
  - `Release`：≤ 20 分钟
  - Release arm64 build step：≤ 10 分钟

## 目标 / 非目标

### Goals

- 降低 `CI (PR)` 的 wall-clock（尤其是“后端改动为主”的 PR），并减少重复构建造成的无效耗时。
- 保持 CI 信心：默认不降低既有质量门槛；如需用“更快”换“覆盖率”，必须显式选择并写入验收标准。
- 让“为什么慢、慢在哪”可观测：至少能区分 queue time 与 job/step runtime，并能看到 cache hit/miss 与其收益。

### Non-goals

- 不改 Dockrev 业务功能、API 语义、数据模型。
- 不切换 CI 平台（GitHub Actions 之外）；
- 不在未明确同意前引入付费/自建 runner（可作为候选方案，但不默认纳入交付范围）。

## 用户与场景

- 维护者：希望 PR 能更快拿到可靠的 CI 结论；在频繁 push/rebase 时不被慢 CI 拖累。
- 贡献者：希望“改后端 ≠ 必跑整套前端 E2E/Storybook”；“改 Docker/Release 相关”才触发对应重型校验。
- 发布流程：希望 Release 工作流仍然稳定、可重跑，并且在 cache warm 后构建时间显著下降。

## 范围（Scope）

### In scope

- 调整/重构 `.github/workflows/ci-pr.yml`、`.github/workflows/ci-main.yml`、`.github/workflows/release.yml` 以减少重复工作并提升缓存命中率。
-（可选）仅为提速目的，对 `Dockerfile` 做**等价**改造（例如启用 BuildKit cache mounts）；前提是不会改变最终产物行为与对外契约。
- 补充与本计划相关的文档：说明 CI 的 job 划分、触发条件、cache 策略与验收口径。

### Out of scope

- 改造编译系统/引入全新构建工具链（例如 Bazel）；
- 把“性能优化”扩展为“全套 CI/CD 体系重做”（除非另立计划）。

## 需求（Requirements）

### MUST

- 给出可选的优化方案（至少 3 档：保守/均衡/激进），每档都必须明确：
  - 具体会改哪些 workflow/job/step；
  - 预期收益（影响面与收益来源：减少重复、cache 提升、跳过重型步骤等）；
  - 风险与回滚策略（例如：gating 可能漏检、cache 失效、基础设施波动等）。
- 提供可验证的验收标准：能够通过 GitHub Actions 的运行结果与日志直接验证（不依赖“本地猜测”）。
- 保持 Release 触发语义不变：仍由 `CI (main)` 成功后触发（`workflow_run`），且在 cache 相关基础设施异常时有可诊断的错误输出。
- 若采用 job gating（基于变更路径决定是否运行重型 job），必须把 gating 规则写入“接口契约”，并提供覆盖边界（例如 `.github/**`、`Dockerfile` 变更时如何处理）。

## 接口契约（Interfaces & Contracts）

本计划把“CI workflows 的触发/分工/gating/cache 行为”视为内部接口（file format contract），以便后续计划与实现保持一致。

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CI workflow: PR build gating & job split | File format | internal | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | contributors | `.github/workflows/ci-pr.yml` |
| CI workflow: main build job composition | File format | internal | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | maintainers | `.github/workflows/ci-main.yml` |
| Release workflow: docker/build cache semantics | File format | internal | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | release pipeline | `.github/workflows/release.yml` |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/file-formats.md](./contracts/file-formats.md)

## 验收标准（Acceptance Criteria）

> 说明：本计划提供多档方案；最终“跑哪些重型校验”取决于主人选择。以下验收分为“通用”与“按方案”的两层。

### 通用（所有方案都必须满足）

- Given 任意 PR 指向 `main`
  When `CI (PR)` 运行
  Then workflow 必须成功结束，且保留后端质量门槛（Rust fmt/clippy/test 等）的结论可追溯。

- Given 任意合并到 `main`
  When `CI (main)` 成功并触发 `Release`（`workflow_run`）
  Then Release 工作流行为与既有口径一致，且失败时输出清晰错误信息（能定位到具体 step）。

### 按方案（由主人选择后冻结）

- Given `Release` workflow 运行
  When 产出 `linux/arm64` release binary
  Then 构建必须在原生 arm64 runner 上完成（避免 QEMU 下容器内编译），且不应再出现“arm64 明显慢于 amd64 数十分钟”的情况（以 baseline 作为对比基线）。

- Given 一个 PR 仅修改后端代码（例如 `crates/**`、`src/**`），且不涉及 `web/**`、`Dockerfile`、`.github/**`
  When `CI (PR)` 运行
  Then（若选择“均衡/激进” gating）前端重型 job（Storybook/Playwright）应被跳过；整体 wall-clock 应较当前明显下降（以主人提供的 baseline 作为对比基线）。

- Given 连续两次在相同依赖输入下运行 Docker build（未改动 `Dockerfile` 与锁文件）
  When 第二次运行触发 `docker/build-push-action`
  Then（若选择启用 build cache）日志中应出现 cache hit/复用迹象，且该 step 的 runtime 显著低于冷启动（以 baseline 对比）。

### 时间目标（冷 cache；主人已接受）

- Given 一个“后端改动为主”的 PR（不触发前端重型检查；`Release build check (PR)` 仍会运行，且 docker 仅做轻量打包校验）
  When `CI (PR)` 运行
  Then workflow wall-clock ≤ 6 分钟。

- Given 合并到 `main`
  When `Release` workflow 运行
  Then workflow wall-clock ≤ 20 分钟，且 arm64 构建相关步骤（在原生 arm64 runner）≤ 10 分钟。

## 实现前置条件（Definition of Ready / Preconditions）

- baseline 已收集并写入本计划（`Release` run `21197273148`、`CI (PR)` run `21196665469`、`CI (main)` run `21197239891`）。
- 关键取舍已确认：native arm runner、PR gating、Docker 产物优先、best-effort build cache、时间目标。
- 接口契约已定稿：见 `./contracts/file-formats.md`，实现与测试可直接按契约落地。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- 需要至少验证两类 PR：
  1) 仅后端改动（验证 gating 生效、后端检查仍完整）
  2) 仅前端改动（验证前端 job 仍会运行且通过）
- 若本计划涉及 Docker build cache：至少验证一次“重复运行”的 cache 命中行为（可用 re-run jobs）。

### Quality checks

- 不引入新质量工具；复用仓库既有 CI 质量检查（Rust fmt/clippy/test、前端 lint/Storybook 测试、Docker build 校验）。

## 文档更新（Docs to Update）

- `docs/plan/0005:github-actions-performance/PLAN.md`: 实施后更新 `Status`、`Last`、验收口径与里程碑。
-（如涉及开发者体验变化）`README.md`: 补充“CI 结构/哪些改动会触发哪些 job”的说明。

## 实现里程碑（Milestones）

（用于驱动 Index 的 `部分完成（x/y）`：只统计本节的 checkbox。）

- [ ] M1: `CI (PR)` 引入变更检测并对前端重型检查做 gating（后端改动不跑 Storybook/Playwright）
- [ ] M2: `CI (PR)` 对 `Release build check (PR)` 做 gating（仅在影响镜像/发布链路的变更时运行）
- [ ] M3: `CI (PR)`/`CI (main)` 去除后端 job 的 web build 依赖（保持 Rust 检查与测试结论不变）
- [ ] M4: `Release` 使用原生 `ubuntu-24.04-arm` runner 构建 arm64 产物（移除 QEMU 下 arm64 编译路径）
- [ ] M5: Docker 改为“产物优先”打包（workflow 产出二进制与 `web/dist`；docker build 只 COPY）
- [ ] M6: Docker build 启用 buildx layer cache（GHA backend），并确保 cache 写入失败不致命
- [ ] M7: 验证并记录提速收益（对照 baseline runs；更新 `Last` 与 Notes）

## 方案概述（Approach, high-level）

已选择：方案 B（均衡，推荐）。

### 方案 A（保守）：不改覆盖面，主要靠缓存与去重提速

- 核心动作：
  - 为 Docker build 引入 Buildx layer cache（GHA cache backend），减少每次从零构建。
  - 去掉明显的重复构建（例如“先本机 build，再 Dockerfile 再 build”中的可替代部分），或让其共享产物/缓存。
  - 评估并减少 Rust job 内的重复编译（例如 `cargo clippy` 与 `cargo check` 的重复；可用 `--locked` 对齐语义）。
  - 为 Playwright 浏览器下载引入缓存（避免每次 `playwright install` 都重新下载）。
- 风险：较低；主要风险在 cache 不稳定/缓存体积过大导致 save/restore 反而变慢。

### 方案 B（均衡，推荐）：按变更路径做 job gating + 关键缓存

- 核心动作：
  - 把 CI 明确拆成 backend / frontend / docker 三类 job，并用变更路径决定是否运行 frontend 与 docker（规则写入契约）。
  - 后端 job 不再执行 `npm ci` / web build（理由：`build.rs` 已提供 placeholder，后端不强依赖 web/dist）。
  - Docker build 校验只在“可能影响镜像”的改动时运行（例如 `Dockerfile`、`.github/**`、`crates/**`、`src/**`、`deploy/**` 等）。
  - 为 Docker build 引入 layer cache（同方案 A）。
  - Release 的 arm64 产物在原生 `ubuntu-24.04-arm` runner 构建（避免 QEMU 编译）。
  - Docker 采用“产物优先”打包：workflow 先构建 `web/dist` 并在编译二进制时嵌入，然后把目标平台二进制写入 `dist/ci/docker/dockrev`；`Dockerfile` 仅 COPY 打包。
- 风险：中等；需要认真设计 path rules，避免漏检。

### 产物传递（Artifacts）

为支撑“产物优先”打包，本计划约定产物在 workflow/jobs 间的传递方式与目录形状（实现需按此契约落地；细节见 `./contracts/file-formats.md`）。

- 默认策略：尽量在**同一 job**内完成“构建产物 + docker build”，避免跨 job 传递导致的复杂度与串货风险。
- 必须跨 job 的场景（例如：汇总 release assets、或在不同 runner 间共享 `web/dist` 用于嵌入编译）：
  - 使用 `actions/upload-artifact@v4` / `actions/download-artifact@v4` 传递产物。
  - artifact 命名需包含 `${{ github.sha }}`（或 `${{ github.run_id }}`），download 后再解包到固定目录。
- 推荐目录约定（示例）：
  - `dist/ci/docker/dockrev`：目标平台的 `dockrev`（musl）二进制（docker build 的唯一必需输入）
  - `dist/ci/web-dist/`：存放 `web/dist` 内容（可选；用于跨 job/runner 复用以加速编译）

### 方案 C（激进）：进一步用“分层 CI”换取更短 PR 时间

- 可能动作（需主人明确同意）：
  - 将最重的检查（例如前端 E2E、Rust `--all-features` 全量检查、multi-arch 相关）从 PR 移到：
    - `main` 合并后执行，或
    - 定时任务（nightly），或
    - 手动触发（workflow_dispatch）。
  - 引入更强的编译缓存（例如 `sccache` / 专用 rust cache action），以及更大规格 runner（付费/自建）。
- 风险：较高；需要明确“哪些覆盖可以晚一点发现”。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：
  - 过度缓存（例如缓存 `target/`）可能导致 save/restore 很慢，反而拉长总时间，需要用数据验证。
  - job gating 规则若不严谨，可能导致“实际受影响的检查被跳过”。
  - Docker/GHA cache 基础设施偶发故障（例如 5xx）会影响 cache-to，需明确是否允许“cache 写失败但 build 继续”。
  - Docker 产物优先打包的负面影响（需要知情同意）：
    - Dockerfile 不再“从源码自给自足”地产出镜像：本地 `docker build` 需要先生成产物（需提供一键脚本/Make target 作为补偿）。
    - 工作流复杂度更高（多 job + 产物传递 + 多架构镜像 manifest），需要更明确的可观测性与失败定位。
    - 需要确保“产物与源码一致”（同一 commit / 同一版本号）；最好在同一 job 中构建+打包，跨 job 时用 artifact 命名约束来避免串货。

- 需要决策的问题：
  - None

- 假设（需主人确认）：
  - CI 主要慢在：重复的 web build + Storybook/Playwright + Docker build（尤其是缺少 layer cache 时）。
  - `ubuntu-24.04-arm` runner label 在目标仓库可用；若不可用则 fallback 到 `ubuntu-22.04-arm` 或改为自建 runner。

## 变更记录（Change log）

- 2026-01-21: 创建计划，完成静态勘察并整理可选提速方案与待决策项。
- 2026-01-21: 冻结关键取舍与时间目标，状态切换为 `待实现`。

## 参考（References）

- `.github/workflows/ci-pr.yml`
- `.github/workflows/ci-main.yml`
- `.github/workflows/release.yml`
- `.github/scripts/compute-version.sh`
- `.github/scripts/smoke-test.sh`
- `Dockerfile`
- `crates/dockrev-api/build.rs`
