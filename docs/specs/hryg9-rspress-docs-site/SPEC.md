# Dockrev：Rspress 完整文档站（中英双语 + Pages）（#hryg9）

## 状态

- Status: 已完成
- Created: 2026-02-26
- Last: 2026-02-26

## 背景 / 问题陈述

- 当前仓库文档分散在 `README.md`、`deploy/README.md` 与 `docs/plan/**`、`docs/specs/**`，面向使用者和运维者的路径不集中。
- 项目缺少统一站点形态与持续发布链路，导致“部署、使用、排障、接口”信息难以持续维护。

## 目标 / 非目标

### Goals

- 新建独立文档工程 `docs-site/`，使用 Rspress 承载中英双语文档站。
- 文档覆盖部署、配置、功能使用、运维、自升级、GHCR webhook、故障排查、FAQ、术语、全量 API 清单。
- 新增 GitHub Pages 自动发布工作流，默认使用 github.io 域名。
- 根 `README.md` 与 `deploy/README.md` 收敛为“快速入口 + 文档站跳转”。

### Non-goals

- 不变更业务代码行为（API、DB、UI 逻辑不变）。
- 不将 `docs/plan/**`、`docs/specs/**` 纳入文档站导航。
- 不引入自定义域名、DNS 配置或 API 自动生成器。

## 范围（Scope）

### In scope

- `docs-site/` 工程初始化与 Rspress 配置（`root/base/locales/nav/sidebar`）。
- 中英双语页面内容落地（同 slug、同层级、同顺序）。
- API 参考页覆盖 `crates/dockrev-api/src/api/mod.rs` 与 `crates/dockrev-supervisor/src/app.rs` 暴露路由。
- `.github/workflows/docs-pages.yml` 构建与部署流程。
- 文档入口更新（`README.md`、`deploy/README.md`）。

### Out of scope

- 现有运行配置、发布流程语义与服务端行为变更。
- 迁移/删除历史规格文档。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `docs-site/` 文档工程 | File format | internal | New | None | docs | maintainers / users | Rspress 站点源文件 |
| `docs:dev/docs:build/docs:preview` | CLI | internal | New | None | docs | maintainers | 根 `package.json` 委托命令 |
| `.github/workflows/docs-pages.yml` | CI workflow | internal | New | None | docs | maintainers | GitHub Pages 自动发布 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 仓库根目录，When 执行 `bun run docs:dev`，Then 文档站可启动并可访问中英入口。
- Given 文档工程，When 执行 `bun run docs:build`，Then 成功生成静态构建产物。
- Given 业务与运维人员，When 查看文档站，Then 可找到完整章节：Quick Start、Deploy、Config、User Guide、Operations、Integrations、API、Troubleshooting、FAQ、Glossary。
- Given API 参考，When 对照 `api/mod.rs` 与 `app.rs`，Then 路由均被覆盖且包含方法、路径、用途、鉴权、关键状态码。
- Given `main` 分支，When 触发 `docs-pages` workflow，Then 可自动部署到默认 GitHub Pages 路径。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bun install --cwd docs-site`
- `bun run docs:build`
- 文档死链与导航可达性人工检查（中英双语）。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `docs-site/` 初始化 + Rspress 配置 + 双语导航。
- [x] M2: 中文完整文档正文（全量运维 + 使用）。
- [x] M3: 英文镜像文档与中英路径对齐。
- [x] M4: 全量 API 参考清单落地。
- [x] M5: Pages workflow 与 README 入口收敛。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：API 人工维护存在漏项风险，需以路由源文件做交叉核对。
- 假设：GitHub Pages 使用默认域名即可满足首版上线要求。
