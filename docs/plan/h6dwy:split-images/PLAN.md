# CI/CD: 拆分 Dockrev / dockrev-supervisor 镜像（#h6dwy）

## 背景 / 问题

当前生产部署里，`dockrev` 与 `dockrev-supervisor` 复用同一个镜像（同仓库同 tag，通过 `command` 选择二进制）。这会让“更新 supervisor（latest）”与“Dockrev 版本意图（由 compose 声明）”在运维心智上产生耦合与误解，也会在某些情况下引发联动升级的风险。

本计划的目标是将交付物拆分为两个独立镜像，使两者的发布/拉取/更新互不影响，同时保持现有单镜像路径的向后兼容。

## 目标（Goals）

- 发布一个新的 supervisor 镜像仓库：`ghcr.io/ivanli-cn/dockrev-supervisor`
- supervisor 镜像默认启动 `dockrev-supervisor`（无需额外 `command`）
- 现有 Dockrev 镜像 `ghcr.io/ivanli-cn/dockrev` 保持不变（仍可运行 `dockrev`，且可选保留 supervisor 二进制以兼容历史部署）
- Release workflow 在同一版本号下同时发布两个镜像（tag + latest 策略见验收标准）

## 非目标（Non-goals）

- 不更改 Dockrev self-upgrade 的“标签范围/策略”定义
- 不引入新的部署配置管理机制（不要求写回 compose/.env，不要求运维新增 override 参数）
- 不改变现有 API/UI 的行为与路由

## 范围（Scope）

### In scope

- Dockerfile 增加 supervisor 独立镜像 target（artifact-first 形态）
- GitHub Actions `Release` workflow 同步构建并推送 `dockrev` 与 `dockrev-supervisor` 两个镜像
- `deploy/docker-compose.yml` 与相关 README 更新为“分镜像”示例
- `README.md` 更新“Releases / Images”口径

### Out of scope

- 重新设计版本选择 UI（例如限制输入 tag）
- 更改 supervisor 对 target compose 的发现/消歧逻辑

## 验收标准（Acceptance Criteria）

- Given `Release` workflow 触发并产出版本 `X.Y.Z`
  - When workflow 完成
  - Then GHCR 上存在：
    - `ghcr.io/ivanli-cn/dockrev:X.Y.Z`（保持现有行为）
    - `ghcr.io/ivanli-cn/dockrev-supervisor:X.Y.Z`
  - And supervisor 镜像同时推送 `:latest`（用于生产“总是最新执行器”的部署意图）
- Given 使用 `deploy/docker-compose.yml` 的分镜像示例
  - When `docker compose up -d`
  - Then supervisor 容器使用 `dockrev-supervisor` 镜像且无需覆写 `command`
  - And Dockrev 容器使用 `dockrev` 镜像

## 测试与验证（Testing）

- 本地：`cargo test -p dockrev-supervisor`
- 本地：`cargo test -p dockrev-api`
- CI（Release）：增加最小 smoke（至少验证 supervisor 镜像内二进制可执行，且 HTTP 端口可监听/路由返回 401/200 符合预期）

## 文档更新（Docs to Update）

- `README.md`：Images 口径从“单镜像”更新为“两镜像”
- `deploy/README.md`：更新示例 compose 说明

## 里程碑（Milestones）

- [ ] M1: Dockerfile 增加 supervisor 镜像 target（prebuilt）
- [ ] M2: Release workflow 推送 `dockrev-supervisor` 镜像（含 tag + latest）
- [ ] M3: 更新 deploy 示例与 README 口径

## 风险 / 备注（Risks / Notes）

- 新增镜像仓库会带来额外的发布与扫描成本；但可显著降低运维歧义与联动升级风险。
- 需要确保 supervisor `latest` 与 Dockrev 的交互契约在“跨 1 个 major”范围内兼容（发布契约）。

