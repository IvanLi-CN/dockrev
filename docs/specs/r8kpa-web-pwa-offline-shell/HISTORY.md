# Dockrev：Web PWA 离线壳、更新提示与分级缓存历史（#r8kpa）

## History

- 2026-07-08: 创建 app-level PWA/offline 规格，定义 installability、离线只读路由边界、IndexedDB 持久快照、7 天过期门控与手动更新提示合同。
- 2026-07-08: 落地 PWA app shell、manifest/icons、全局 SW 注册与手动更新提示；完成首页旧快照迁移桥、统一只读快照层、首页/运维大盘/队列/版本推测/Stack 详情的本地快照优先加载，并补充 Storybook 视觉证据。
- 2026-07-08: 补齐 service detail 的离线只读子页：`overview / monitoring / backup` 走 IndexedDB 快照与本地监控样本回放，`logs / settings` 明确保留联网门控，并新增对应 Storybook 覆盖与视觉证据。
- 2026-07-08: 将离线展示策略收紧为仅允许 `fresh` 快照；离开新鲜窗口的本地数据不再展示，相关页面文案与 Storybook 场景同步移除“数据过时”提示。
- 2026-08-08: 更新切换改为后台完整预缓存后的导航切版。以 Workbox `waiting` 作为 ready 门禁，加入下载/失败状态与 single-flight 激活；更新提示迁移为不占文档流的响应式浮动气泡，并将 app-shell fallback 扩展到全部正式前端路由。
- 2026-08-18: 将 install icon 拆为保持原有外观的 regular，以及独立的不透明 maskable/Apple touch 输出；平台外壳不再由资产模拟，内容版本 URL 与 PWA precache 一起维护。
