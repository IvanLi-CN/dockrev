# 文件格式（File formats）

将“GitHub Actions workflows 的 job 划分 / gating / cache 语义”视为一种内部接口契约来描述。

## CI workflows：gating 与 job 分工（`.github/workflows/*.yml`）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 编码（Encoding）: utf-8 (yaml)

### 背景（现状要点）

- `CI (PR)`：`.github/workflows/ci-pr.yml`
  - `lint`：包含 Rust fmt/clippy/check，以及前端 lint + Storybook + Playwright 测试
  - `unit-tests`：Rust tests（当前也会执行 web build）
  - `pr-release-check`：musl release build + smoke + Docker build 校验（当前会执行 web build）
- `CI (main)`：`.github/workflows/ci-main.yml`
  - 结构与 `CI (PR)` 类似（无 `pr-release-check`）

### Schema（结构）— 目标形状（按方案选择冻结）

> 注意：本节描述的是“我们允许的契约形状”。本计划已选择方案 B；A/C 仅作为备选参考。

#### 方案 A（保守）

- 所有既有 job 仍会运行（不改变覆盖面），但允许：
  - 为 Docker build 引入 buildx layer cache（见下文）。
  - 为 Playwright 浏览器下载引入缓存（见下文）。
  - 在不改变语义前提下删除明显重复步骤（例如重复的 web build / 可合并的 Rust 检查）。

#### 方案 B（均衡，推荐）

- `CI (PR)` 允许拆分为三类 job：
  - `backend`（always）：Rust fmt/clippy/test（不再执行 `npm ci` / web build）
  - `frontend`（conditional）：web lint + Storybook + Playwright（仅当 web 相关变更）
  - `docker`（conditional）：Docker build 校验（仅当会影响镜像的变更）
- `CI (main)` 同理（通常不需要 `docker` 校验，除非主人希望 main 也跑）。

#### 方案 C（激进）

- 在方案 B 基础上，允许把最重的检查迁移到 main/nightly/manual（需主人明确同意，并在 `PLAN.md` 验收中写清楚）。

### Gating 规则（建议；需主人确认后冻结）

> 本计划已确认允许对 PR 做 gating。这里冻结“哪些变更会触发哪些 job”的规则（后续实现必须遵守）。

- `frontend`（Storybook/Playwright）触发规则（任一满足即运行）：
  - 变更路径命中：`web/**`
- `Release build check (PR)` 触发规则（任一满足即运行；否则可跳过）：
  - `Dockerfile`
  - `.github/**`（含 `.github/scripts/**` 与 `.github/workflows/**`）
  - `deploy/**`
  - `Cargo.toml`、`Cargo.lock`、`crates/**`、`src/**`
  - `web/**`（因为 UI 会被嵌入到后端二进制中）

### 兼容性与迁移（Compatibility / migration）

- job gating 属于“覆盖面变更”：必须写入本契约与 `PLAN.md` 的验收标准；如需回滚，应能够一键恢复为“全部 job 都跑”的形态。
- 对于 cache：
  - 缓存 key 需包含锁文件（`Cargo.lock`、`web/package-lock.json`）；
  - 已确认：cache 失败（尤其是 cache export）允许继续构建（best-effort；例如允许 `ignore-error=true`）。

## Docker build：Buildx layer cache 语义（`docker/build-push-action`）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 编码（Encoding）: n/a（YAML inputs）

### Schema（结构）

- Docker build 允许使用 Buildx 的 cache backends（推荐 GitHub Actions cache backend）。
- 典型形状（示意，非最终实现）：

```yaml
- name: Build (with cache)
  uses: docker/build-push-action@v6
  with:
    context: .
    file: ./Dockerfile
    platforms: ${{ env.PLATFORMS }}
    push: false # or true in Release
    cache-from: type=gha
    cache-to: type=gha,mode=max
```

### 风险与策略

- GitHub Cache Service 偶发故障会导致 `cache-to` 失败：
  - 已确认：cache 写失败不应让 build 失败（best-effort cache）。
  - 实现可选策略（以“只忽略 cache export 错误、不要忽略 build 错误”为原则）：
    - 直接在 cache exporter 层启用 ignore-error（若底层 backend 支持该参数）；
    - 或在检测到“仅 cache export 失败”的情况下做一次无 cache-to 的回退重跑（避免吞掉真实构建失败）。

## Playwright：浏览器下载缓存语义（`~/.cache/ms-playwright`）

- 范围（Scope）: internal
- 变更（Change）: Modify

### Schema（结构）

- 允许将 Playwright browser 下载目录纳入缓存，以减少 `playwright install` 的下载成本：
  - 典型路径：`~/.cache/ms-playwright`
  - key 需包含 `web/package-lock.json`（或 Playwright 版本）以避免不匹配。

### Open decisions（待主人确认）

- None

## Docker：产物优先打包（artifact-first）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 编码（Encoding）: utf-8 (dockerfile + yaml)

### Schema（结构）

- `Dockerfile` 在 CI/CD 中不再负责编译 Rust/web：
  - web UI 通过 `crates/dockrev-api/src/ui.rs` 的 `include_dir!("$OUT_DIR/dockrev-ui-dist")` 嵌入到二进制中；
  - 因此 workflow 必须在 `cargo build --release` 前先完成 `web` 的 build（或接受 placeholder UI）。
- 对于 docker build（打包阶段），约定输入产物路径为：
  - `dist/ci/docker/dockrev`：目标平台的 `dockrev`（musl）二进制（单文件）
  - docker build 仅 COPY 该文件进镜像（以及保留现有 docker/compose CLI 的打包逻辑）。

### 产物传递（Artifacts passing）

- 同一 job 内：直接把构建产物写入 `dist/ci/docker/dockrev` 后执行 docker build（推荐，最简单）。
- 跨 job（例如 arm64/amd64 并行构建、或汇总发布产物）：
  - 使用 `actions/upload-artifact@v4` 上传 `dist/ci/docker/` 或 release tarballs；
  - 使用 `actions/download-artifact@v4` 下载并解包到同样的固定路径，再执行 docker build 或 release 上传。

## Release：arm64 原生构建（避免 QEMU）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 编码（Encoding）: utf-8 (yaml)

### Schema（结构）

- `linux/arm64` 产物（release binaries、以及可能的 docker image 相关产物）应在原生 arm64 runner 上构建，避免 QEMU（例如 `docker run --platform linux/arm64 ...`）导致的超长编译时间。
- 目标 runner（GitHub-hosted arm64 standard runner）：
  - 优先：`runs-on: ubuntu-24.04-arm`
  - 备选：`runs-on: ubuntu-22.04-arm`
