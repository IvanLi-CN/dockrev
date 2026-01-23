# CLI Contracts（#0002）

本文件定义 Storybook 相关的本地命令接口（开发者入口）。执行目录均为 `web/`。

## Commands

### `npm run storybook`

- Purpose: 启动 Storybook 开发服务器（本地预览与交互调试）。
- Working dir: `web/`
- Output:
  - stdout 打印本地访问地址（例如 `http://localhost:<port>`）
- Exit codes:
  - `0`: 正常退出（例如手动停止）
  - `!= 0`: 启动或运行失败（配置/依赖/编译错误等）
- Default port:
  - `50886`（可用 `DOCKREV_STORYBOOK_PORT` 覆盖，`--port` 优先）

### `npm run build-storybook`

- Purpose: 构建静态 Storybook（用于离线预览/未来部署）。
- Working dir: `web/`
- Output:
  - 默认输出目录：`web/storybook-static/`（如 Storybook 默认行为有变化，以实现时实际配置为准）
- Exit codes:
  - `0`: 构建成功
  - `!= 0`: 构建失败

## Pass-through options

- 允许通过 npm 的 `--` 透传参数到底层命令（例如自定义端口）。
  - 示例：`npm run storybook -- --port 6007`

### `npm run test-storybook`

- Purpose: 运行 Storybook 自动化测试（用于 CI / 本地验证）。
- Working dir: `web/`
- Target selection:
  - 默认：对本地 Storybook（例如 `http://127.0.0.1:50887`）运行测试（实现阶段需确保命令行为稳定）
  - 可选：通过 `--url` 或环境变量 `TARGET_URL` 指向已部署的 Storybook 实例
    - 示例：`npm run test-storybook -- --url https://the-storybook-url-here.com`
    - 示例：`TARGET_URL=https://the-storybook-url-here.com npm run test-storybook`
- Output:
  - stdout 打印测试用例执行结果
- Exit codes:
  - `0`: 测试通过
  - `!= 0`: 测试失败或运行错误
