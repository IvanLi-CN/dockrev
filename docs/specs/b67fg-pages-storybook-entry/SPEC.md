# Dockrev：GitHub Pages 合并发布文档站与 Storybook（#b67fg）

## 状态

- Status: 已完成
- Created: 2026-03-07
- Last: 2026-03-07

## 背景 / 问题陈述

- 当前 GitHub Pages 仅发布 `docs-site/` 的 Rspress 文档站，公开 Storybook 入口不存在。
- `docs-pages` workflow 仍是单 job 串行构建，文档与 Storybook 无法并行，Pages 构建等待时间偏长。
- 文档首页与顶栏缺少直接跳转 Storybook 的入口，UI 预览与使用文档分离，不利于评审和对外展示。

## 目标 / 非目标

### Goals

- 保持单一 GitHub Pages 站点，在文档站同域同 base 下新增公开 Storybook 路径 `/storybook/`。
- 将 `docs-pages` workflow 改为 `build-docs`、`build-storybook`、`assemble-pages`、`deploy` 四段，其中前两段并行构建。
- 新增复用型组装脚本，将 docs 构建产物放站点根目录、Storybook 构建产物放 `storybook/` 子目录。
- 在 docs 顶栏、默认首页、中文首页、英文首页都新增 Storybook 跳转入口。
- 保持 `DOCS_BASE` 对站内绝对链接的重写语义，不在文档里写死仓库名。

### Non-goals

- 不拆分独立 Storybook 站点、独立仓库、独立域名或第二个 Pages workflow。
- 不扩展 README 在线入口。
- 不修改 Storybook stories、组件实现或生产运行时行为。

## 范围（Scope）

### In scope

- `.github/workflows/docs-pages.yml`
- `.github/scripts/assemble-pages-site.sh`
- `docs-site/rspress.config.ts`
- `docs-site/docs/index.md`
- `docs-site/docs/zh/index.md`
- `docs-site/docs/en/index.md`
- `docs-site/docs/storybook.mdx`
- `docs/specs/README.md`

### Out of scope

- `README.md`
- `web/src/**`
- Storybook 配置文件与现有 stories 内容

## 接口契约（Interfaces & Contracts）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 备注（Notes） |
| --- | --- | --- | --- | --- |
| `/storybook/` | Public URL | external | New | GitHub Pages 上公开的 Storybook 入口 |
| `docs-pages` workflow | CI workflow | internal | Modify | 改为并行构建 + 汇总后部署 |
| `assemble-pages-site.sh` | CLI | internal | New | 组装 docs 根目录 + `storybook/` 子目录 |
| docs 顶栏 / 首页入口 | Docs navigation | internal | Modify | 统一跳到 `storybook.html` redirect 页面，再跳转到 `/storybook/` |

## 验收标准（Acceptance Criteria）

- Given `pull_request` 命中 Pages 相关路径，When workflow 运行，Then `build-docs` 与 `build-storybook` 并行执行，随后进入 `assemble-pages`，且不执行部署。
- Given `push main` 或 `workflow_dispatch`，When workflow 运行，Then 仅在前三个 job 成功后执行 `deploy`。
- Given 组装后的站点目录，When 检查文件结构，Then 站点根下存在 `index.html`，且存在 `storybook/index.html`。
- Given docs 顶栏与首页入口，When 在 GitHub Pages repo base 下访问，Then 先进入 `/<repo>/storybook.html` redirect 页面，再跳转到 `/<repo>/storybook/`。
- Given Storybook 作为子路径部署，When 打开 `/storybook/`，Then `iframe.html`、`index.json`、`assets/*` 与 `sb-manager/*` 均可加载，无资源 404。

## 非功能性验收 / 质量门槛（Quality Gates）

- `bun run docs:build`
- `bun run --cwd web build-storybook -- --quiet`
- 本地组装后访问 docs 根页与 `storybook.html` redirect 页面，确认可跳转到 `/storybook/` 且静态资源可用。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新建 spec，冻结单站点 `/storybook/` 入口与并行构建口径。
- [x] M2: docs 顶栏、默认/中/英文首页与 `storybook.html` redirect 页面完成落地。
- [x] M3: 新增 Pages 组装脚本并改造 workflow 为并行构建 + 汇总后部署。
- [x] M4: 完成本地验证与 spec sync，确保组合站点可访问。

## 变更记录（Change log）

- 2026-03-07：创建规格，锁定 GitHub Pages 单站点发布 Storybook 的路径、入口位置与 workflow 并行构建方案。
- 2026-03-07：完成 docs 顶栏/首页入口、`storybook.html` redirect 页面、Pages 组装脚本与并行 workflow 改造；本地通过 docs build、Storybook build、组装烟测与浏览器跳转验证。
