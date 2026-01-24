# Supervisor CLI contract (internal)

目标：supervisor 在本地执行“拉取 Dockrev 镜像、应用更新、等待健康、失败回滚”的最小命令集。

说明：该 CLI 为 internal contract，用于实现与测试对齐；对外只暴露 `contracts/http-apis.md` 的 API。

## Inputs

- Compose 入口：
  - `composeProject`（例如 `dockrev`）
  - `composeFiles`（一组路径；优先来自目标容器的 `com.docker.compose.project.config_files`）
  - `serviceName`（Dockrev 在 compose 中的 service 名）
- Docker 镜像目标：
  - `imageRef`（固定为 Dockrev 镜像仓库）
  - `targetTag` 或 `targetDigest`
- 超时与等待：
  - `pullTimeoutSeconds`
  - `healthyTimeoutSeconds`
- 回滚策略：
  - `rollbackOnFailure`（true/false）

## Commands (conceptual)

1. Precheck（读取当前运行版本与 digest、检查 docker/compose 可用）
2. Pull（拉取目标镜像 tag/digest）
3. Apply（`docker compose up -d --pull always` 或等价流程）
4. Wait healthy（轮询 Dockrev 的 `/api/health` 或容器 healthcheck）
5. Postcheck（确认 `/api/version` 或 supervisor 观测到的新 digest）
6. Rollback（失败时回滚到 previous digest，并再次 wait healthy）

## Compose discovery (recommended)

- 通过 Docker inspect 读取目标 Dockrev 容器 labels：
  - `com.docker.compose.project` → `composeProject`
  - `com.docker.compose.project.config_files` → `composeFiles`（逗号分隔的绝对路径）
- 若 label 缺失或 `config_files` 路径在 supervisor 侧不可读：必须退回到显式配置（见 `contracts/config.md`），并返回可行动错误提示（例如“请将 compose 目录以相同绝对路径只读挂载到 supervisor”）。

## Exit semantics

- 成功：返回 `succeeded`（并输出 structured logs）
- 失败：
  - 若回滚成功：返回 `rolled_back`（并输出失败原因与回滚结果）
  - 若回滚失败：返回 `failed`（需要人工介入）
