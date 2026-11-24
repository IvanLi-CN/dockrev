# Dockrev：Cleanup 快照优先与即时确认弹窗

> 本文是清理页快照读取、确认弹窗、前台自动刷新与执行准入的长期行为合同。实现覆盖见 `IMPLEMENTATION.md`，主题生命周期与兼容背景见 `HISTORY.md`。

## Context and Scope

- Context: 清理页已经由服务端持久化清理快照并执行 300 秒新鲜度校验，但页面打开和每次动作点击都会强制全量扫描，确认弹窗必须等待扫描完成后才出现。
- In scope: `/cleanup` 页面加载策略、`全部 / 清理此 stack / 清理此服务` 的即时弹窗、扫描中的发现态、前台自动刷新、后台扫描期间的 apply 准入、过期与失败处理、动效降级。
- Out of scope: 扫描算法本身的性能改造（并行 `docker inspect`）、300 秒新鲜度常量、自动执行清理、Docker prune 语义、非清理页面。

## Terms and Interfaces

- `cleanup inventory snapshot`: 一次清理扫描得到的全部候选及其扫描时间、估算口径；页面与操作都投影自同一份快照。
- `snapshot freshness window`: 快照扫描时间之后 300 秒内可作为执行依据的固定窗口。
- `partial cleanup snapshot`: 扫描进行中产生的、仅含已发现候选的展示用快照；不能作为执行依据。
- `cleanup discovery state`: 确认弹窗在没有可确认快照时进入的展示状态；只呈现当前扫描已发现的候选，不能批准清理。
- `foreground cleanup refresh`: 清理页在前台且快照接近新鲜度边界时，由前端发起的自动清理扫描。
- `snapshotAgeMs`: 扫描响应中由服务端计算的非负快照年龄，用于客户端判断新鲜度与调度，不依赖浏览器时钟。
- `scanDurationMs`: 客户端从发起扫描到收到 ready 的实测耗时；用于计算下一次自动刷新的提前量。

## Interfaces

| Interface | Kind | Change | Contract |
| --- | --- | --- | --- |
| `POST /api/cleanups/scan` | HTTP API | Modify | [http-apis.md](./contracts/http-apis.md) |
| `POST /api/cleanups/apply` | HTTP API | Modify | [http-apis.md](./contracts/http-apis.md) |
| `CleanupScanResponse` | Shared payload | Modify | [http-apis.md](./contracts/http-apis.md) |
| `POST /api/cleanups/scan-runs` + SSE | HTTP API | Modify | [http-apis.md](./contracts/http-apis.md) |
| `CleanupPage` interaction controller | Web module | New | [http-apis.md](./contracts/http-apis.md) |

## Requirements

### REQ-CSI-001

- 清理页加载 MUST 先读取服务端已有快照，且 MUST NOT 在快照仍新鲜时强制启动扫描。
- 快照缺失或过期时，客户端 MUST 启动至多一次清理扫描；已有旧快照可以继续展示，但 MUST 标记为过期且 MUST NOT 作为执行依据。
- 扫描运行期间页面 MUST 渲染已有快照，而不是把 pending 当成空页面。

### REQ-CSI-002

- 点击 `全部`、`清理此 stack` 或 `清理此服务` MUST 在同一次交互内立即打开确认弹窗，MUST NOT 等待网络请求或扫描完成。
- 当前快照新鲜时，弹窗 MUST 直接使用内存中的快照与 fingerprint 进入可确认状态。
- 当前快照过期或缺失时，弹窗 MUST 进入 `cleanup discovery state`，不得先展示可确认的旧候选。

### REQ-CSI-003

- 发现态 MUST 展示固定阶段进度：容器、镜像、卷、网络、构建缓存；并展示当前阶段、已发现候选数量和已知字节小计。
- 已到达的 `partial cleanup snapshot` MUST 渐进填入候选列表，而不是等 ready 后一次性出现。
- 发现态的计数和字节 MUST 明确标注为“已发现”或“部分”，MUST NOT 标注为最终预计释放空间。
- ready 之前确认操作 MUST 保持禁用。

### REQ-CSI-004

- `partial cleanup snapshot` MUST 仅用于展示；客户端 MUST NOT 用其生成或提交 `confirmationFingerprint`。
- 服务端 MUST 拒绝没有 ready fingerprint 的 cleanup apply。

### REQ-CSI-005

- 确认弹窗使用的快照 MUST 只在新鲜度窗口内可执行；超过窗口后 MUST 立即切换到发现态，丢弃可执行状态并启动一次扫描。
- 任何过期快照 MUST NOT 用于批准清理，MUST NOT 以“再点一次就提交”的方式保留。
- 页面上的过期快照 MUST 标记为过期；动作入口点击后进入发现态，而不是直接确认。
- 客户端判定年龄 MUST 使用 `snapshotAgeMs + 响应接收后经过的本地时间`，MUST NOT 使用绝对本地时钟直接减 `scannedAt`。

### REQ-CSI-006

- 当 `/cleanup` 可见且用户最近一次交互在 30 分钟内时，客户端 MUST 在快照年龄达到提前量时启动一次 `foreground cleanup refresh`。
- 提前量 MUST 为 `clamp(lastScanDurationMs + 15000, 60000, 240000)`；没有历史耗时时 MUST 使用 60000。
- 交互 MUST 以 `pointerdown`、`keydown`、`wheel`、`touchstart` 为准，MUST NOT 把 `mousemove` 算作交互。
- 确认弹窗打开期间 MUST 暂停调度；弹窗关闭后 MUST 立即重新评估。
- 页面不可见时 MUST 停止调度；重新可见时 MUST 立即评估一次。
- 连续 30 分钟没有交互后 MUST 停止自动调度；下一次交互 MUST 恢复评估。
- 前台刷新运行期间，基于仍新鲜快照的动作入口 MUST 保持可用；扫描运行本身 MUST NOT 单独禁用确认或提交。
- 同一时刻 MUST 只有一条活动清理扫描；重复触发 MUST 加入现有扫描。

### REQ-CSI-007

- `POST /api/cleanups/apply` MUST 接受仍然新鲜的快照，即使后台扫描正在运行。
- apply MUST NOT 等待后台扫描结束，也 MUST NOT 仅因扫描运行中而拒绝。
- apply MUST 继续校验新鲜度、fingerprint、volume 实例身份，并与 Stack 记录删除保持原子性。
- 上一次扫描失败 MUST NOT 阻止基于仍新鲜快照的 apply。

### REQ-CSI-008

- apply 返回 `409 cleanup_snapshot_stale` 且携带 ready 快照时，弹窗 MUST 原地替换候选、标记“候选已变化”并要求再次确认。
- apply 返回 stale 且携带 pending 时，弹窗 MUST 进入发现态。
- 客户端 MUST NOT 自动重试 apply。

### REQ-CSI-009

- 发现态扫描失败时，弹窗 MUST 丢弃已发现的部分候选，显示稳定的失败状态与重试入口，并保持确认禁用。
- 前台自动刷新失败时，页面 MUST 保留最后一次成功快照并标记过期；重试 MUST 按 1、2、4、8、16、30 分钟指数退避。
- 自动刷新成功或用户手动 `重扫` 后，退避 MUST 重置。
- 手动 `重扫` 的渐进流式更新与稳定错误状态 MUST 保持不变。

### REQ-CSI-010

- 发现态允许关闭弹窗；关闭 MUST NOT 取消正在进行的扫描。
- 扫描继续时页面 MUST 继续显示进度；再次打开动作弹窗 MUST 加入同一扫描并显示当前部分或 ready 结果。

### REQ-CSI-011

- 首次进入且没有任何快照时，`全部` MUST 可点击并打开发现态弹窗。
- Stack 行与服务行只在存在至少一份快照后出现；该场景不影响 `全部` 的发现态入口。

### REQ-CSI-012

- 发现态 MUST 使用阶段切换、候选渐进插入和计数变化表达进展。
- 动效 MUST 遵守 `prefers-reduced-motion`；降级时 MUST 保留静态阶段、计数与结果。
- 发现阶段 MUST NOT 对“预计释放空间”做持续跳动或最终值暗示。

### REQ-CSI-013

- 所有入口（页面加载、前台刷新、手动重扫、弹窗发现态）MUST 共享服务端单飞扫描；客户端 MUST NOT 并发启动多条清理扫描。
- 多标签页 MUST 复用同一活动扫描与同一份服务端快照。

### REQ-CSI-014

- `POST /api/cleanups/scan` 与 scan-run 事件中的 ready、partial 响应 MUST 包含 `snapshotAgeMs`；pending 响应 MUST 为 `null`。
- scan-run 的 partial 事件 MUST 按有界节奏推送：距上次推送至少 300 毫秒或累计 25 个候选，且每个资源阶段结束时必须推送一次。
- 既有 `status`、`refreshing`、`retryAfterMs`、`confirmationFingerprint` 字段语义 MUST 保持不变。

### REQ-CSI-015

- 前台自动刷新 MUST 静默进行，只在 ready 后整体替换页面快照；MUST NOT 把 partial 渐进写入页面。
- 手动 `重扫` MUST 继续按 partial 事件渐进替换页面。

## Verification

### VER-CSI-001

- Method: `cargo test -p dockrev-api cleanup_`，覆盖 fresh page scan、stale/missing page scan、confirm 与 apply 的新鲜度分支。
- covers: `REQ-CSI-001`, `REQ-CSI-005`, `REQ-CSI-007`
- Pass condition: 新鲜快照不触发新扫描；过期快照触发至多一次扫描；扫描运行期间 apply 仍可基于新鲜快照成功，过期或 fingerprint 不匹配时返回 409。

### VER-CSI-002

- Method: `cargo test -p dockrev-api cleanup_` 并发用例，覆盖 apply 与扫描最终写入、Stack 记录删除的交错。
- covers: `REQ-CSI-007`, `REQ-CSI-013`
- Pass condition: apply 不被整轮扫描阻塞；Stack 删除完成后的扫描不会复活旧归属快照；同一时刻只产生一条扫描。

### VER-CSI-003

- Method: web 单元测试覆盖新鲜度、提前量、退避与交互计时的纯函数。
- covers: `REQ-CSI-006`, `REQ-CSI-009`, `REQ-CSI-014`
- Pass condition: 提前量在 60–240 秒之间按最近耗时计算；30 分钟无交互停止、交互后恢复；退避序列正确；`snapshotAgeMs` 缺失时不误判为新鲜。

### VER-CSI-004

- Method: Storybook interaction test，覆盖 fresh snapshot 即时弹窗、发现态、阶段切换、过期切换、失败清空、关闭后重开、窄屏与 reduced motion。
- covers: `REQ-CSI-002`, `REQ-CSI-003`, `REQ-CSI-004`, `REQ-CSI-008`, `REQ-CSI-010`, `REQ-CSI-011`, `REQ-CSI-012`
- Pass condition: 点击后同一帧出现弹窗；发现态无可执行确认；partial 不产生 fingerprint；过期后旧候选不可确认；失败后候选清空；重开加入同一扫描；reduced motion 下无位移或无限动画。

### VER-CSI-005

- Method: HTTP 契约测试读取 scan 与 scan-run 响应，校验新增字段与 partial 节奏。
- covers: `REQ-CSI-014`
- Pass condition: ready/partial 均含非负 `snapshotAgeMs`，pending 为 null；partial 推送不早于 300 毫秒或 25 个候选，且阶段结束必有一次。

### VER-CSI-006

- Method: 前端集成测试把快照年龄推进到 301 秒并尝试确认。
- covers: `REQ-CSI-005`
- Pass condition: 弹窗切换为发现态，确认入口不可用，旧 fingerprint 不会被提交。

### VER-CSI-007

- Method: web 集成测试注入 partial 与 ready 事件，分别覆盖前台自动刷新和手动重扫。
- covers: `REQ-CSI-015`
- Pass condition: 自动刷新在 ready 前不改变页面快照；手动重扫在 partial 到达时渐进更新页面。

## Related ADRs

- [ADR 0023：Cleanup 读取已有快照](../../adr/0023-cleanup-reads-existing-snapshot.md)

## Visual Evidence

- None

## References

- [Docker Prune 清理控制台](../qynjg-docker-prune-cleanup-console/SPEC.md)
- [Cleanup apply 指纹稳定化](../hw3pb-cleanup-apply-fingerprint-stability/SPEC.md)
- `./IMPLEMENTATION.md`
- `./HISTORY.md`
