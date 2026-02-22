# Dockrev Web: 统一候选版本展示优先使用 resolvedTag（#rsezg）

## 背景

线上接口已返回候选推断版本（`candidate.resolvedTag`），但 Web 多处仍直接渲染 `candidate.tag`（例如 `latest`），导致“候选版本号推断正确但界面仍显示 latest”的错觉与误导。

## 目标

- 统一候选版本展示策略：优先展示 `candidate.resolvedTag`，缺失时回退 `candidate.tag`。
- 保持现有后端契约不变，仅修复前端展示层。

## 非目标

- 不改后端扫描/推断逻辑。
- 不改 API 字段结构或命名。
- 不改 `VersionTagsPopover` 的查询参数语义（仍基于 raw tag + digest）。

## 范围

### In

- `web/src/pages/OverviewPage.tsx`
- `web/src/pages/ServicesPage.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- 新增前端共享 helper（统一 current/candidate 的显示规则）

### Out

- `crates/**`（Rust API/DB/scan）
- 任务执行逻辑（update/check/runtime-scan）

## 验收标准

1. 当 `candidate.tag=latest` 且 `candidate.resolvedTag=v0.2.51` 时，Overview/Services/ServiceDetail 均显示 `v0.2.51`。
2. 当 `candidate.resolvedTag` 缺失或不是 semver 时，UI 回退显示 raw `candidate.tag`。
3. `VersionTagsPopover` 仍可正常打开并展示 digest/tags 详情。
4. 不出现 TypeScript 构建错误。

## 测试

- 新增前端单测覆盖候选显示函数（resolved 优先、回退 raw、非法 resolved 回退）。
- 运行 `web` 构建验证：`bun run build`。

## 风险

- 各页面历史上存在重复实现，若仅修一处易再次分叉；本计划通过共享 helper 消除此风险。
- 若未来 semver 判定规则调整，需要修改共享 helper 并补充对应测试。

## 里程碑

- [x] M1: docs freeze（计划与索引入库）
- [x] M2: 统一 helper 与页面替换完成
- [x] M3: 单测与构建通过
- [x] M4: PR 创建并 checks 结果明确
