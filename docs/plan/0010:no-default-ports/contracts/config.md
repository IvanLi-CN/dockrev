# Config（No Default Ports / #0010）

本文件定义本计划新增/变更的配置项（主要为环境变量），用于统一“高位端口”策略。

## Environment variables

### `DOCKREV_WEB_DEV_PORT`

- Type: integer
- Default: `50884`
- Meaning: `web` 的 Vite dev server 默认端口（替代默认 `5173`）。

### `DOCKREV_WEB_PREVIEW_PORT`

- Type: integer
- Default: `50885`
- Meaning: `web` 的 Vite preview 默认端口（替代默认 `4173`）。

### `DOCKREV_STORYBOOK_PORT`

- Type: integer
- Default: `50886`
- Meaning: `web` 的 Storybook dev 默认端口（替代默认 `6006`）。

### `DOCKREV_TEST_STORYBOOK_PORT`

- Type: integer
- Default: `50887`
- Meaning: `web` 的 `test-storybook` 在“未指定 URL 自起本地 server”模式下使用的默认端口（替代 `6006`）。

## Related existing config (non-exhaustive)

- API listen addr: `DOCKREV_HTTP_ADDR`（当前默认已为高位端口：`0.0.0.0:50883`）
- Web API proxy target: `VITE_API_PROXY_TARGET`（当前默认指向 `http://127.0.0.1:50883`）
