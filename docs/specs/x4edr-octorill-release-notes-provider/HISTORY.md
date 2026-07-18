# Dockrev：OctoRill 更新日志来源与发布抽屉视图切换 演进历史（#x4edr）

> 这里记录会影响 Agent 理解“为什么一步步变成现在这样”的关键演进；单次任务流水账不放这里，规范正文仍以 `./SPEC.md` 为准。

## Decision Trace

- 2026-07-07: 创建 spec，冻结 v1 只读接入 OctoRill feed、默认 `smart` 视图与 API Key 后端代理边界。
- 2026-07-07: OctoRill API Key 脱敏回显改为等长圆点串，保留明文不回传的安全边界，同时避免用户误判已保存 key 被截短。
- 2026-07-12: 服务更新记录扩展为既有发布抽屉的第二入口；继续复用统一 release notes API、来源切换、定位与虚拟滚动，不新增并行 viewer。
- 2026-07-16: 统一 release notes API 响应新增仓库级 `externalLinks`，让服务详情版本页与发布抽屉都能直接打开 GitHub / OctoRill Releases 列表，而不在前端重复猜 URL。
- 2026-07-17: GitHub Releases fallback 在发布抽屉与服务详情 `版本` 子页改为安全 Markdown 渲染，保留标题、列表、强调与 compare 链接语义，不再把 `##` / `*` 原样暴露给用户。
- 2026-07-18: 统一 release notes 运行时选源收口到 Settings `releaseNotes.provider`；删除 OctoRill 失败后自动改用 GitHub 的契约，改为“设成啥用啥 + 同源 stale-only”。

## Key Reasons / Replacements

- 复用既有 GitHub Releases 抽屉作为承载面，避免新增并行 release viewer。
- API Key 仅由 Dockrev 后端保存与转发，避免浏览器直连第三方时泄漏敏感凭据。
- 脱敏回显允许暴露 key 字符长度，但不暴露明文内容；Settings 保存路径必须把全星号或全圆点掩码视为保留旧 key。
- Release Notes provider 一旦进入 Settings，就必须成为所有入口共享的唯一真相源；否则抽屉、版本页与其它入口会因局部 override 出现不可解释的跨源结果。
- OctoRill 文档仅声明 `translated` / `smart` 存在，未稳定展开内部字段，因此实现必须宽容解析并允许缺失降级。
- GitHub Releases 和 OctoRill 的正文都可能是 Markdown 源文本；阅读面需要保留结构化语义，但不能引入原始 HTML 执行面。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
