# History

- 2026-06-26: Created the follow-up spec to freeze the EdgeOne `15s` origin timeout constraint, snapshot-backed cleanup/deploy-check reads, release drawer fallback, and `5s` SSE heartbeat policy.
- 2026-06-26: Implemented cleanup snapshot workers, deploy-check refresh workers, owner-facing API contract changes, Web polling flows, Storybook coverage, and visual evidence.
- Deploy-check now acts as a hard capability gate on startup and foreground resume; cached preference settings cannot bypass a required-core failure.
- Deploy-check pass/fail desktop and `393x852` mobile mock evidence now covers the hard gate and Dashboard lock; the full Storybook smoke suite is green.
- Required core checks now require explicit `pass`, and App-level startup failure stories verify that `neverAutoOpen` cannot bypass the deploy-check gate.
- Discovery reconciliation is performed by an effective scan rather than database startup. Healthy saved Compose files report `stopped`; all-file `ENOENT` uses `auto_archive_compose_files_missing`; partial absence and unreadable or invalid files remain visible as `invalid`.
- System archives created by the current or legacy automatic reason recover when a valid scan proves the project is stopped or invalid. User archives, Stack metadata, Service metadata, and running resources remain unchanged.
- Shared-testbox cleanup remains relative to the verified run directory after containment checks, so a later parent-path replacement cannot redirect deletion outside that run scope.
- 管理页面改用应用级 SSE：缓存 deploy-check PASS 立即放行并后台复核；事件仅保存在 60 秒或 1024 条的进程内环形缓冲，断线通过游标补发或 REST 重同步恢复，且不保留轮询降级。
