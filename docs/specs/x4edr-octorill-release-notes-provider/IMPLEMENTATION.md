# Dockrev：OctoRill 更新日志来源与发布抽屉视图切换 实现状态（#x4edr）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现
- Lifecycle: implemented
- Catalog note: fast-track（OctoRill release notes provider + Settings 配置 + release drawer 视图切换）

## Coverage / rollout summary

- 后端 settings schema/API 已支持 `releaseNotes.provider`，并继续保留 `releaseNotes.octoRill` 的 Base URL 规范化、API Key 等长脱敏、保留/覆盖/清除语义与默认视图；运行时选源只看 provider。
- 新增服务级 `GET /api/services/{service_id}/release-notes`，只请求 Settings 当前选中的单一 provider，失败时不再跨源回退。
- 统一 release notes 响应现已补齐 `externalLinks.githubReleasesUrl` 与可选 `externalLinks.octoRillReleasesUrl`，供版本页和发布抽屉共享仓库级 Releases 入口。
- 新增服务级 `GET /api/services/{service_id}/release-notes/locate`，首屏由后端直接返回锚点窗口、`previousCursor` 与结构化 `anchor`；普通 list 路径支持 `direction=older|newer` 双向续拉。
- OctoRill locate 优先复用 public releases highlight/window 能力；当前实例无法提供目标窗口时，统一返回 OctoRill 失败或 unavailable 锚点，不再改用 GitHub items。
- 发布抽屉与服务详情 `版本` 子页改用统一 release notes API，显示来源、stale banner，并支持会话内同源旧结果复用；GitHub provider 只暴露 `original`。
- 发布抽屉与服务详情 `版本` 子页的 release card 现已对 GitHub Releases / OctoRill 的 Markdown 正文做安全渲染，保留标题、列表、强调、链接与显式换行；原始 HTML 仍不会执行。
- 服务更新记录与服务详情 `版本` 子页都改为 locate-first：首屏只渲染锚点窗口，不再为了“定位当前版本”在线性翻页中自动扫完整个版本历史。
- Settings 新增统一 release notes 配置卡片，复用现有自动保存状态与错误提示。
- Storybook mock API、Settings story 与 Release Drawer story 已覆盖 provider 配置、等长 API Key 脱敏、默认润色、GitHub 单视图与 stale 分支。

## Remaining Gaps

- v1 不触发 OctoRill 翻译/润色生成接口；缺失内容只做只读降级。
- OctoRill `translated` / `smart` 字段结构按宽容解析处理，后续若第三方契约稳定可收紧类型。

## Verification

- `cargo check -p dockrev-api`
- `cargo test -p dockrev-api api::services::release_notes::tests -- --nocapture`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun test web/tests/serviceVersionCardMarkdown.test.tsx web/tests/releaseNotesLayout.test.ts`
- `bun test web/tests/releaseDrawer.test.ts`
- Storybook static + Playwright screenshot capture for `components-githubreleasedrawer--anonymous-located` and `components-githubreleasedrawer--outside-window`

## Related Changes

- `crates/dockrev-api/src/api/services/release_notes.rs`
- `crates/dockrev-api/src/api/types/settings.rs`
- `crates/dockrev-api/src/api/types/mod.rs`
- `crates/dockrev-api/src/api/types/services.rs`
- `web/src/components/GitHubReleaseDrawer.tsx`
- `web/src/components/ServiceVersionCard.tsx`
- `web/src/pages/SettingsPage.tsx`
- `web/src/stories/pages/serviceDetailPageStoryFixtures.ts`
- `web/src/stories/pages/serviceDetailVersionsStories.tsx`
- `web/src/stories/components/GitHubReleaseDrawer.stories.tsx`
- `web/src/stories/pages/SettingsPage.stories.tsx`
- `web/tests/serviceVersionCardMarkdown.test.tsx`

## References

- `./SPEC.md`
- `./HISTORY.md`
