# 命令行（CLI）

## `web`：安装依赖

- 范围（Scope）: internal
- 变更（Change）: Modify

### 用法（Usage）

```text
cd web
bun install
```

### 约束（Constraints）

- CI 应使用 frozen lockfile（确保可复现安装）。
- 不要求 npm 与 node 存在。

## `web`：CI 安装依赖

- 范围（Scope）: internal
- 变更（Change）: New

### 用法（Usage）

```text
cd web
bun install --frozen-lockfile
```

### 约束（Constraints）

- 用于 CI 的可复现安装；锁文件不匹配应严格失败。

## `web`：运行 scripts

- 范围（Scope）: internal
- 变更（Change）: Modify

### 用法（Usage）

```text
cd web
bun run <script> [script-flags...]
```

### 约束（Constraints）

- scripts 名称保持与现有一致（例如 `lint` / `build` / `storybook` / `build-storybook` / `test-storybook`），仅替换执行入口。
- 不应依赖 `node` 二进制；脚本应在 Bun runtime 下可执行。

### 参数透传（Pass-through）

- `bun run <script> [script-flags...]` 会把 `script-flags` 直接传给脚本命令；不同于 npm，通常不需要额外的 `--` 分隔符。
