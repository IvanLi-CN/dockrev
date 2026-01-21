# 文件格式（File formats）

将“发布产物的命名与标签规则”视为一种接口契约来描述。

## GHCR 镜像命名与 tag 规则（`ghcr.io/<owner>/dockrev:<tag>`）

- 范围（Scope）: external
- 变更（Change）: Modify
- 编码（Encoding）: n/a

### Schema（结构）

- Registry: `ghcr.io`
- Image name: `<owner>/dockrev`（`<owner>` 为 GitHub repository owner 的小写形式）
- Tags:
  - 版本 tag：`<APP_EFFECTIVE_VERSION>`（格式：`MAJOR.MINOR.PATCH`）
  - `latest`：是否更新取决于触发来源（见下文“触发与发布语义”）
- Platforms（多架构）: `linux/amd64,linux/arm64`
- OCI labels（最小集合）:
  - `org.opencontainers.image.version=<APP_EFFECTIVE_VERSION>`
  - `org.opencontainers.image.revision=<git sha>`
  - `org.opencontainers.image.source=https://github.com/<owner>/<repo>`

### Examples（示例）

- `ghcr.io/ivanli-cn/dockrev:0.3.12`
- `ghcr.io/ivanli-cn/dockrev:latest`

### 兼容性与迁移（Compatibility / migration）

- 新增 tags/labels 应保持向后兼容（添加式变更）。
- 删除/重命名 tag 口径属于破坏性变更，需在计划中明确弃用周期与迁移建议。

## 触发与发布语义（`.github/workflows/release.yml`）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 编码（Encoding）: utf-8 (yaml)

### Schema（结构）

发布工作流仅允许一条触发路径：

1) `workflow_run`（自动发布路径）
   - 触发源：`CI (main)` 在 `main` 成功完成
   - 行为：推送 `${APP_EFFECTIVE_VERSION}`，并（当前实现）推送 `latest`

### 同步语义（Sync semantics）

- 是否允许“半发布”：不允许。
- 原子性边界：先确保 GHCR 镜像构建+推送成功，再创建/更新 GitHub Release（含 assets）。
- 失败策略（fail-fast / ordering）：
  - 镜像构建或推送失败：工作流失败结束；不得创建/更新 GitHub Release。
  - 镜像推送成功后，若 Release/资产上传失败：工作流失败；允许镜像残留（不做清理/回滚），但必须输出清晰错误说明（失败步骤/原因/重试建议）。

### Open decisions（待主人确认）

- None
