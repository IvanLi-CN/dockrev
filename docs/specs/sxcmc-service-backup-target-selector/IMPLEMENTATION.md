# Dockrev：服务保护设置补全备份目标直选与备份说明 实现状态（#sxcmc）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现
- Lifecycle: ready-for-pr
- Catalog note: fast-track（service backup target API + service backup records API + Service detail backup tab + Storybook evidence）

## Coverage / rollout summary

- 已新增 `GET/PUT /api/services/{service_id}/backup-targets`，返回服务级 bind path / volume 候选、共享信息与只读备份存储说明。
- 已新增 `GET /api/services/{service_id}/backup-records`，返回“当前服务相关”的实际备份记录、计划删除时间与资产明细，并收紧为只返回真正产出过备份产物的记录；没有 `artifactPath` 的 `skipped` / `failed` 尝试都会被排除，`assets[]` 也只保留实际纳入备份的 target。
- 已扩展 compose 解析链路，按 compose 声明提取 named volumes 与 bind mounts，并把相对 bind path 解析为基于 compose 文件目录的绝对路径。
- 已将服务级备份策略落到独立关系表，并在服务端把它投影回现有 settings 视图，兼容当前其余读取路径。
- 备份存储由 `dirname(DOCKREV_DB_PATH)/backups` 派生；Docker 模式通过 API 容器 mount inspect 映射实际 bind 或 named volume，无法唯一解析时拒绝执行。
- 新归档由内置 helper 以 `GNU tar stdout -> zstd stdin` 单管道写入 `.tar.zst.part`，完成后原子 rename；旧 `.tar.gz` 记录继续可读。
- 更新事务先完成镜像 pull 与备份准备。需要停服时只停止原本运行的相关服务，归档后更新目标直接 apply，其他目标立即恢复；在线备份与 pull 并行。
- 停机前持久化实际运行服务与临时产物 checkpoint；Dockrev 重启会先停止对应 stop-mode helper、清理 `.part` 并只恢复停机前运行的服务，在线 helper 不受该恢复流程终止。
- helper JSONL 每约 500ms 转换为非持久化 SSE 与终端快照，数据库进度按 checkpoint 节流，任务总进度覆盖 pull、backup、apply 与 finalize。
- 已实现共享 target 的保守取消语义：独占 target 取消时从 `stack.backup.targets` 移除；共享 target 取消时仅把当前服务 policy 记为 `disabled`。
- 已保留现有 `PUT /api/services/{service_id}/settings` 作为服务设置保存链路，不再让它承担 stack 级 backup target 管理副作用。
- 已更新升级前备份执行链路：`停机备份` 会协调停掉相关服务后备份并恢复，`在线备份` 保持相关服务运行后直接备份。
- 已把备份相关入口迁移到服务详情 `备份` 子页，设置页仅保留回滚和代码仓库配置。
- 已更新 Service Detail 的“备份设置”抽屉，支持 `Volumes` / `Bind paths` 直选、三策略按钮组（`不备份 / 停机备份 / 在线备份`）、共享提示、空态文案与只读备份说明区块。
- 已更新 Storybook mock API 与页面故事，覆盖 backup tab 的有记录/空态、备份设置入口、共享关闭、无候选与只读说明等关键状态。

## Related Changes

- Backend: service backup-target API、compose mount parsing、stack/service backup target reconciliation、API tests。
- Web: client types、ServiceDetail state/source、备份子页与备份设置抽屉 UI、样式、Storybook mock handlers 与页面故事。
- Docs: topic-level spec、implementation/history、视觉证据资产。

## References

- `./SPEC.md`
- `./HISTORY.md`
