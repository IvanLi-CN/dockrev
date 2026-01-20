# CLI Contracts（#0003）

本文件定义“自动发布链路”中被 CI 调用的脚本接口（面向 CI/维护者），用于稳定地产出版本号口径。

## Commands

### `.github/scripts/compute-version.sh`

- Purpose: 计算本次发布的有效版本号，并导出到环境变量 `APP_EFFECTIVE_VERSION`。
- Working dir: repo root（或任意 workdir，只要 `git rev-parse --show-toplevel` 指向仓库根目录）。
- Inputs:
  - `Cargo.toml` 中的 `version`（形如 `x.y.z`）
  - 已存在的 git tags（形如 `v<semver>`）
- Behavior:
  - 以 `Cargo.toml` 的 `major.minor.patch` 作为 base
  - 若 `v<major>.<minor>.<patch>` 已存在，则递增 patch，直到找到一个未被占用的版本
- Outputs:
  - 向 `$GITHUB_ENV` 写入：`APP_EFFECTIVE_VERSION=<semver>`
  - stdout 打印 computed version（用于日志）
- Exit codes:
  - `0`: 计算成功
  - `!= 0`: 计算失败（例如无法读取 `Cargo.toml` 版本号）

### `.github/scripts/smoke-test.sh` (planned)

- Purpose: 在 CI 中对“构建产物”做最小运行验证（启动服务端→探活→访问 web）。
- Working dir: repo root
- Inputs:
  - `DOCKREV_SMOKE_BIN` (required): 要测试的二进制路径（例如 `./target/release/dockrev`）
  - `DOCKREV_SMOKE_ADDR` (optional, default `127.0.0.1:50883`): 服务监听地址
  - `DOCKREV_SMOKE_TIMEOUT_SECONDS` (optional, default `20`): 等待就绪超时时间
  - `APP_EFFECTIVE_VERSION` (optional): 若提供，则校验 `GET /api/version` 与其一致
  - Runtime env（最小集合；具体以实现冻结为准）：
    - `DOCKREV_HTTP_ADDR`：由脚本设置为 `DOCKREV_SMOKE_ADDR`
    - `DOCKREV_DB_PATH`：指向临时文件（例如 `$RUNNER_TEMP/dockrev.sqlite3`）
- Behavior:
  - 启动 `dockrev` 并等待就绪
  - 必须通过：
    - `GET /api/health` → `200` 且 body 为 `ok`
    - `GET /` → `200` 且返回 HTML（可用 `<!doctype html>` 作为最小断言）
    - （可选）`GET /api/version` → `200` 且 `version` 与 `APP_EFFECTIVE_VERSION` 一致
  - 结束时必须清理子进程（即使失败也要退出前 kill）
- Output:
  - stdout 打印探测过程与失败原因（便于 CI 排障）
- Exit codes:
  - `0`: smoke test 通过
  - `!= 0`: smoke test 失败（启动失败/超时/端点不符合契约）
