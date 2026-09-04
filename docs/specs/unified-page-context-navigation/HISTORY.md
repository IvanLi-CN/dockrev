# Dockrev 统一页面内导航演进历史

## Decision Trace

- 以单一侧栏收敛一级页面导航和页面内目录，消除桌面双栏与详情三栏的重复壳层。
- 页面内目录只组合当前页面已有读模型；清理筛选明确为视图投影，避免把展示状态泄漏到执行请求。
- 保留旧导航主题目录、历史和视觉资产，目录索引以 successor 关系指向本主题，便于追溯已替换的合同。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- `docs/specs/2jhm2-app-shell-sidebar-collapse-icons/HISTORY.md`
- `docs/specs/c2r2u-detail-route-service-navigation/HISTORY.md`
