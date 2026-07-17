# Dockrev：OctoRill 更新日志来源与发布抽屉视图切换 实现状态（#x4edr）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现
- Lifecycle: implemented
- Catalog note: fast-track（OctoRill release notes provider + Settings 配置 + release drawer 视图切换）

## Coverage / rollout summary

- 后端 settings schema/API 已支持 `releaseNotes.octoRill`，包含 Base URL 规范化、API Key 等长脱敏、保留/覆盖/清除语义与默认视图。
- 新增服务级 `GET /api/services/{service_id}/release-notes`，开启且配置完整时优先请求 OctoRill repo feed，失败时返回 fallback 并回退 GitHub Releases。
- 统一 release notes 响应现已补齐 `externalLinks.githubReleasesUrl` 与可选 `externalLinks.octoRillReleasesUrl`，供版本页和发布抽屉共享仓库级 Releases 入口。
- 新增服务级 `GET /api/services/{service_id}/release-notes/locate`，首屏由后端直接返回锚点窗口、`previousCursor` 与结构化 `anchor`；普通 list 路径支持 `direction=older|newer` 双向续拉。
- OctoRill locate 优先复用 public releases highlight/window 能力；当前实例无法提供目标窗口时，统一回退 GitHub items，并通过 `fallback` 与 `anchor.message` 显式说明失去 `smart / translated`。
- 发布抽屉改用统一 release notes API，显示来源、fallback banner，并支持 `润色 / 翻译 / 原文` 会话内切换。
- 服务更新记录与服务详情 `版本` 子页都改为 locate-first：首屏只渲染锚点窗口，不再为了“定位当前版本”在线性翻页中自动扫完整个版本历史。
- Settings 新增 OctoRill 更新日志配置卡片，复用现有自动保存状态与错误提示。
- Storybook mock API、Settings story 与 Release Drawer story 已覆盖配置卡、等长 API Key 脱敏、默认润色、视图切换与 fallback 分支。

## Remaining Gaps

- v1 不触发 OctoRill 翻译/润色生成接口；缺失内容只做只读降级。
- OctoRill `translated` / `smart` 字段结构按宽容解析处理，后续若第三方契约稳定可收紧类型。

## Verification

- `cargo check -p dockrev-api`
- `cargo test -p dockrev-api release_notes -- --nocapture`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun test web/tests/releaseDrawer.test.ts`
- Storybook static + Playwright screenshot capture for `components-githubreleasedrawer--anonymous-located` and `components-githubreleasedrawer--outside-window`

## Related Changes

- `crates/dockrev-api/src/api/services/release_notes.rs`
- `crates/dockrev-api/src/api/types/settings.rs`
- `crates/dockrev-api/src/api/types/mod.rs`
- `crates/dockrev-api/src/api/types/services.rs`
- `web/src/components/GitHubReleaseDrawer.tsx`
- `web/src/pages/SettingsPage.tsx`
- `web/src/stories/components/GitHubReleaseDrawer.stories.tsx`
- `web/src/stories/pages/SettingsPage.stories.tsx`

## References

- `./SPEC.md`
- `./HISTORY.md`
