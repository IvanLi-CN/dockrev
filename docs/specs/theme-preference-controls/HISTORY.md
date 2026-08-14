# Dockrev 三态主题偏好与响应式入口 演进历史

> 这里记录会影响 Agent 理解“为什么一步步变成现在这样”的关键演进；单次任务流水账不放这里，规范正文仍以 `./SPEC.md` 为准。

## Decision Trace

- 复用已有 `dockrev:theme` 合同：缺失 key 继续代表 system，避免要求 Supervisor 和旧客户端理解新的存储字面量。
- 移动端只在设置区展示入口；桌面端利用侧栏用户区，展开侧栏才显示三选滑块。
- 普通点击按系统当前解析色选择相反显式主题，再切换到匹配显式主题，最后回到 system；直接选择仍通过菜单和滑块提供。

## Key Reasons / Replacements

- 选择缺失即 system 作为持久化边界，保留旧用户的显式 light/dark 偏好并保持同源 Supervisor 回退逻辑。
- 选择侧栏与设置页作为入口位置，是由桌面与移动导航的可用空间差异决定，而不是新增业务导航层。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
