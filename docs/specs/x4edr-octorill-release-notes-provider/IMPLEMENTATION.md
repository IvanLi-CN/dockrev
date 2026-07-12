# Dockrev：OctoRill 更新日志来源与发布抽屉视图切换 实现状态（#x4edr）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现
- Lifecycle: implemented
- Catalog note: fast-track（OctoRill release notes provider + Settings 配置 + release drawer 视图切换）

## Coverage / rollout summary

- 后端 settings schema/API 已支持 `releaseNotes.octoRill`，包含 Base URL 规范化、API Key 等长脱敏、保留/覆盖/清除语义与默认视图。
- 新增服务级 `GET /api/services/{service_id}/release-notes`，开启且配置完整时优先请求 OctoRill repo feed，失败时返回 fallback 并回退 GitHub Releases。
- 发布抽屉改用统一 release notes API，显示来源、fallback banner，并支持 `润色 / 翻译 / 原文` 会话内切换。
- 服务更新记录中的目标版本入口复用同一发布抽屉与统一 release notes API；定位、高亮与虚拟滚动行为不因入口不同而分叉。
- Settings 新增 OctoRill 更新日志配置卡片，复用现有自动保存状态与错误提示。
- Storybook mock API、Settings story 与 Release Drawer story 已覆盖配置卡、等长 API Key 脱敏、默认润色、视图切换与 fallback 分支。

## Remaining Gaps

- v1 不触发 OctoRill 翻译/润色生成接口；缺失内容只做只读降级。
- OctoRill `translated` / `smart` 字段结构按宽容解析处理，后续若第三方契约稳定可收紧类型。

## Verification

- `cargo test -p dockrev-api release_notes -- --nocapture`
- `cargo test -p dockrev-api settings_and_notifications_roundtrip -- --nocapture`
- `cargo test -p dockrev-api`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook`
- `bun run --cwd web storybook:screenshots -- --outdir ../docs/specs/x4edr-octorill-release-notes-provider/assets --only pages-settingspage--octo-rill-release-notes-card,components-githubreleasedrawer--octo-rill-smart-default`

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
