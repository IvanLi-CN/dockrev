# 文件格式（File formats）

## GitHub Release assets: dockrev-supervisor tarball（`dockrev-supervisor_<ver>_linux_<arch>_<libc>.tar.gz`）

- 范围（Scope）: external
- 变更（Change）: New
- 编码（Encoding）: binary（tar.gz）

### Schema（结构）

- Path pattern:
  - `dockrev-supervisor_<ver>_linux_<arch>_<libc>.tar.gz`
  - `dockrev-supervisor_<ver>_linux_<arch>_<libc>.tar.gz.sha256`
- `<ver>`: semver（与 Git tag 一致，例如 `0.3.0`；无 `v` 前缀）
- `<arch>`: `amd64` | `arm64`
- `<libc>`: `gnu` | `musl`
- Tarball content:
  - 顶层文件：`dockrev-supervisor`（可执行文件）
  - 不包含其它附带文件（README、LICENSE 等不在本计划范围内）
- Checksum file:
  - 生成方式与 `dockrev_*` 保持一致（同一命令/输出格式）
  - 语义：对同名 `.tar.gz` 文件的 sha256 校验

### Examples（示例）

- `dockrev-supervisor_0.3.0_linux_amd64_musl.tar.gz`
- `dockrev-supervisor_0.3.0_linux_amd64_musl.tar.gz.sha256`

### 兼容性与迁移（Compatibility / migration）

- 该变更为纯新增（additive）：不移除/重命名现有 `dockrev_*` assets。
- 若未来需要扩展平台（例如 Windows/macOS）或调整内容布局，必须在此契约中先行变更并明确兼容策略。
