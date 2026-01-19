# CLI（Docker Compose runner）

本计划将更新执行以 Compose CLI 为主（internal）。

- 默认 `docker-compose`
- 若宿主机支持 `docker compose` plugin，可切换使用

建议提供一个可配置的 compose 可执行文件名：

- `DOCKREV_COMPOSE_BIN`（默认 `docker-compose`）

## Backup runner（pre-update backup）

- 范围（Scope）: internal
- 变更（Change）: New

目标：

- 对每个备份 target 先做体积估算（bytes）
- 大于阈值则跳过（记录 `skipped_by_size`）
- 其余目标打包到备份产物（默认落在 `DOCKREV_BACKUP_BASE_DIR`）

### Size probe（体积估算）

建议策略：使用一次性容器读取目标数据并执行 `du`，得到近似体积。

- Docker named volume：
  - `docker run --rm -v <volume>:/data:ro alpine sh -lc 'du -sb /data | cut -f1'`
- bind mount（host path）：
  - `docker run --rm -v <hostPath>:/data:ro alpine sh -lc 'du -sb /data | cut -f1'`

Notes:

- 体积估算可能较慢；实现阶段可加入超时与错误策略。
- 估算失败时：默认视为“无法估算”，按 `skipped_by_probe_error` 记录并跳过（不视为备份失败）。
  - 体积估算与备份前都必须应用 service 级 `backupTargets` 过滤：
    - bind mounts：以 host path 匹配 `bindPaths`
    - docker volumes：以 volume name 匹配 `volumeNames`

### Create backup（生成备份）

备份产物建议格式：

- `<baseDir>/<stackId>/<YYYYMMDD-HHMMSSZ>.tar.zst`

建议打包方式（可替换实现，但必须保证可恢复）：

- `tar` 打包目标路径/目录；压缩可选（zstd/gzip/none）
- 产物元信息（stackId、jobId、时间、targets 列表、skipped 列表、size）写入 DB 与 job logs

### Cleanup（清理）

- 更新成功且稳定窗口达到 `deleteAfterStableSeconds` 后，删除对应备份产物，并写入审计日志。

## docker compose（update/apply）

- 范围（Scope）: internal
- 变更（Change）: New

### 约定

- Dockrev 容器内必须能访问：
  - Docker socket（例如 `/var/run/docker.sock`）
  - compose 文件与 `.env` 文件（以“容器内绝对路径”形式注册）
- 多文件 compose：用多个 `-f` 参数（按注册顺序）：
  - `docker compose -f <f1> -f <f2> ...`
- 如提供 `envFile`：使用 `--env-file <path>`；未提供时不传该参数。
- project name：
  - 默认使用 stack 的 `name`（或其稳定派生值）
  - 通过 `--project-name <name>` 传入（避免依赖当前目录推断）

### Apply（按范围）

- 全 stack：
  - `<compose> ... pull`
  - `<compose> ... up -d`
- 单 service：
  - `<compose> ... pull <service>`
  - `<compose> ... up -d <service>`

### Rollback（按 digest/本地镜像）

目标：不改写 compose 文件也能把服务回滚到“更新前”的镜像内容。

建议流程（按 service）：

1) 更新前记录该 service 的 `imageId`（或等价可复用引用）与 tag（来自 compose 配置）。
2) 回滚时将“更新前镜像” retag 回原 tag（避免 compose 拉最新）：
   - `docker image tag <oldImageId> <imageRepo>:<tag>`
3) 执行 `up` 并显式禁止 pull：
   - `<compose> ... up -d --pull never <service>`

Notes:

- 如果 `oldImageId` 不在本机（被清理），回滚将失败；需要在实现中明确“保留镜像”策略。

### Exit codes / errors

- `0`: success
- `!= 0`: failure（stderr 收集到 job logs）

### Observability

- 必须捕获 stdout/stderr 并写入 job logs（支持分页/截断）
- 在日志里标注：scope、compose files、env file、project name、目标 service（如适用）
