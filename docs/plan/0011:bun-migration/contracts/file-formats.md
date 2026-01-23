# File formats

## `bun.lock`

- 范围（Scope）: internal
- 变更（Change）: Modify
- 目的：作为仓库根目录的唯一锁文件（替代 `package-lock.json`），用于可复现安装与 CI 缓存 key。

### 生成与迁移

- 从 npm lockfile 迁移：Bun 可在缺少 `bun.lock` 时自动从 `package-lock.json` 迁移生成（原锁文件保留，待验收后移除）。

## `web/bun.lock`

- 范围（Scope）: internal
- 变更（Change）: Modify
- 目的：作为 `web/` 的唯一锁文件（替代 `web/package-lock.json`），用于可复现安装与 CI 缓存 key。

### 生成与迁移

- 从 npm lockfile 迁移：Bun 可在缺少 `web/bun.lock` 时自动从 `web/package-lock.json` 迁移生成（原锁文件保留，待验收后移除）。
