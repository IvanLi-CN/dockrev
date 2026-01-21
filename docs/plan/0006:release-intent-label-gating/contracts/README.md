# 接口契约（Contracts）

本目录用于存放本计划涉及的**跨边界接口契约**。为避免形状混杂，契约必须按 `Kind` 拆分成不同文件（不要把 CLI / File format 等混在同一文件里）。

编写约定：

- `../PLAN.md` 是唯一的“接口清单（Inventory）”；每条接口都必须在清单里出现，并指向对应契约文件。
- 修改既有接口时，契约里必须写清楚：
  - 变更点（旧 → 新）
  - 向后兼容期望
  - 迁移 / rollout 方案（如适用）

本计划包含：

- `cli.md`：GitHub Actions 内部脚本的输入/输出/退出码约定
- `file-formats.md`：PR 标签集合、版本号/tag 形态、release gating 信号等对外/对内稳定形状
