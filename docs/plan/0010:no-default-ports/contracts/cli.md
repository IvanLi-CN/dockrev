# 命令行（CLI）

本文件定义本计划涉及的 CLI 行为契约（端口默认值、覆盖方式与冲突处理）。

## `web: vite (dev)`

- 范围（Scope）: internal
- 变更（Change）: Modify

### 用法（Usage）

```text
cd web
# bun:
bun run dev [vite-options...]

# legacy (until #0011 is done):
npm run dev -- [vite-options...]
```

### 端口（Port）

- 默认：使用 `DOCKREV_WEB_DEV_PORT`（见 `./config.md`），不得落到 Vite 默认端口 `5173`
- 覆盖：
  - 允许通过 CLI 显式指定端口（例如 `--port <n>`；允许显式指定 `5173`）
  - 建议冲突时优先使用 env vars（见 `./config.md`）显式指定高位端口
  - 优先级：CLI 参数 `--port` > 环境变量 `DOCKREV_WEB_DEV_PORT` > 默认值
- 冲突：端口被占用时严格失败退出（非 0），不得自动换端口继续启动

## `web: vite preview`

- 范围（Scope）: internal
- 变更（Change）: Modify

### 用法（Usage）

```text
cd web
# bun:
bun run preview [vite-preview-options...]

# legacy (until #0011 is done):
npm run preview -- [vite-preview-options...]
```

### 端口（Port）

- 默认：使用 `DOCKREV_WEB_PREVIEW_PORT`，不得落到 Vite preview 默认端口 `4173`
- 覆盖：
  - 允许通过 CLI 显式指定端口（例如 `--port <n>`；允许显式指定 `4173`）
  - 建议冲突时优先使用 env vars（见 `./config.md`）显式指定高位端口
  - 优先级：CLI 参数 `--port` > 环境变量 `DOCKREV_WEB_PREVIEW_PORT` > 默认值
- 冲突：端口被占用时严格失败退出（非 0），不得自动换端口继续启动

## `web: storybook dev`

- 范围（Scope）: internal
- 变更（Change）: Modify

### 用法（Usage）

```text
cd web
# bun:
bun run storybook [storybook-options...]

# legacy (until #0011 is done):
npm run storybook -- [storybook-options...]
```

### 端口（Port）

- 默认：使用 `DOCKREV_STORYBOOK_PORT`，不得落到 Storybook 默认端口 `6006`
- 端口必须以 Storybook CLI 的 `--port` 语义显式指定
- 冲突：优先使用 Storybook 的 `--exact-port` 语义（端口被占用则失败退出）
- 覆盖：
  - 允许显式指定 `--port 6006`（但文档建议优先用 env vars 显式指定高位端口）
  - 优先级：CLI 参数 `--port` > 环境变量 `DOCKREV_STORYBOOK_PORT` > 默认值

## `web: test-storybook`

- 范围（Scope）: internal
- 变更（Change）: Modify

### 用法（Usage）

```text
cd web
# bun:
bun run test-storybook [test-storybook-options...]

# legacy (until #0011 is done):
npm run test-storybook -- [test-storybook-options...]
```

### 端口（Port / URL）

- 若指定 `--url <url>` 或设置 `TARGET_URL`：
  - 不自起本地 server，直接对目标 URL 跑测试
- 否则（默认）：
  - 自起本地静态 server，监听 `DOCKREV_TEST_STORYBOOK_PORT`，不得使用 `6006`
  - 将本地 URL 作为 `--url` 传给 `test-storybook`
  - 优先级：`--url` > `TARGET_URL` > 本地自起

### 退出码（Exit codes）

- `0`: 测试通过
- `!= 0`: 测试失败或运行错误
