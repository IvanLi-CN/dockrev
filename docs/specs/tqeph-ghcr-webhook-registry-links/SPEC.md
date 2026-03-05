# Dockrev：GHCR 维护页项目标题链接与 Webhook 快捷入口（#tqeph）

## 状态

- Status: 已完成
- Created: 2026-03-05
- Last: 2026-03-05

## 背景 / 问题陈述

- GHCR 维护页当前只把仓库名渲染为纯文本，无法直接跳转到 GitHub 仓库页面。
- 行操作区缺少“查看仓库 webhook 网页入口”，遇到排障时需要手工拼接 URL，操作成本高。
- 需要在不改后端 API 的前提下，补齐仓库级外链操作能力并保持现有同步/删除流程不回归。

## 目标 / 非目标

### Goals

- 将 GHCR 维护页每行仓库名称升级为标题风格展示（加粗），并支持新标签页打开仓库地址。
- 在“同步状态”按钮左侧新增“Webhook 页面”按钮。
- Webhook 页面跳转规则固定为：`hookId` 存在跳详情页；否则跳 hooks 列表页。
- 保持现有 API、状态字段与任务动作语义不变。

### Non-goals

- 不新增或修改后端 API、DB schema、worker 行为。
- 不改造 queue/inbox 相关页面。
- 不引入新的权限模型或 GitHub 域名配置能力。

## 范围（Scope）

### In scope

- `web/src/pages/GhcrWebhookRegistryPage.tsx`
- `web/src/App.css`
- `docs/specs/README.md`

### Out of scope

- `crates/dockrev-api/**`
- 任何运行时配置项与路由结构扩展

## 接口契约（Interfaces & Contracts）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 备注（Notes） |
| --- | --- | --- | --- | --- |
| `GitHubPackagesRepo.fullName` | Type | internal | No change | 前端解析 owner/repo 生成 GitHub 链接 |
| `GitHubPackagesRepo.hookId` | Type | internal | No change | 前端决定 webhook 详情/列表 URL |
| `https://github.com/{owner}/{repo}` | External URL | external | New usage | 仓库标题点击跳转 |
| `https://github.com/{owner}/{repo}/settings/hooks[/hookId]` | External URL | external | New usage | 行操作“Webhook 页面”跳转 |

## 验收标准（Acceptance Criteria）

- Given GHCR 维护页有仓库行，When 页面渲染，Then 项目名以标题风格显示并加粗。
- Given 用户点击项目名，When 新标签页打开，Then 跳转到该仓库 `https://github.com/{owner}/{repo}`。
- Given 行操作区渲染，When 查看按钮顺序，Then “Webhook 页面”位于“同步状态”左侧。
- Given `hookId` 存在，When 点击“Webhook 页面”，Then 跳转 `.../settings/hooks/{hookId}`。
- Given `hookId` 为空，When 点击“Webhook 页面”，Then 跳转 `.../settings/hooks`。
- Given 既有同步/删除动作，When 回归验证，Then 行为与禁用条件无回归。

## 非功能性验收 / 质量门槛（Quality Gates）

- `bun run --cwd web lint`
- `bun run --cwd web build`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 仓库标题改为可点击外链，应用标题风格（加粗 + hover/focus 可访问态）。
- [x] M2: 新增“Webhook 页面”按钮并按 `hookId` 规则跳转。
- [x] M3: 移动端与长仓库名样式回归（截断/不换行错位）。
- [x] M4: lint/build 通过并完成规格同步。

## 变更记录（Change log）

- 2026-03-05：创建规格，锁定仓库标题链接与 webhook 快捷入口交互规则。
- 2026-03-05：完成前端实现与样式调整；新增 Storybook `RegistryLinks` 场景覆盖仓库链接与 webhook URL 分支；`bun run --cwd web lint` 与 `bun run --cwd web build` 通过。
