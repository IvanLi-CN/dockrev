# Dockrev：Release Publication 对齐 latest 与 PR 发版评论（#qnq3w）

## 状态

- Status: 部分完成（4/5）
- Created: 2026-03-28
- Last: 2026-03-28

## 背景 / 问题陈述

- 当前 `Release` workflow 的 `latest` 决策仍基于“`main` 上是否存在更晚 stable snapshot”，而不是“是否已经存在更晚 stable 发布记录”。
- 这会让 `0.35.8` / `0.35.9` 这种队列场景在发布时都跳过 `latest`，最终使 `ghcr.io/ivanli-cn/dockrev:latest` 与 `ghcr.io/ivanli-cn/dockrev-supervisor:latest` 停留在更旧版本。
- 当前 release 链路也没有把实际发布出的 `release_tag` 回写到源 PR，导致 PR timeline 无法作为发版审计入口。
- `#48mh8` 把“自动 PR release-version comment upsert”列为 non-goal；该限制仅代表当时范围，不再作为当前 release 语义真相源。本 spec supersede `#48mh8` 中关于 PR 发版评论的非目标描述。

## 目标 / 非目标

### Goals

- 将 `latest` 的定义冻结为“最新已发布 stable 镜像”，而不是“最新 stable snapshot”。
- 为 stable 与 rc 发布新增源 PR issue comment upsert，正文回写实际 `release_tag`、release URL、workflow run URL、channel。
- 引入可审计的发布账本 `refs/notes/release-publications`，让 rerun / backfill 能基于“已发布事实”重新计算 `latest`。
- 保持现有 semver/tag 契约不变：stable 仍为 `<semver>`，rc 仍为 `<semver>-rc.<sha7>`。

### Non-goals

- 不改变 `type:*` / `channel:*` label 契约。
- 不改变 release 触发拓扑、镜像名、GHCR 仓库结构或 release assets 格式。
- 不引入 review comment 作为 PR 发版评论载体。

## 范围（Scope）

### In scope

- `.github/scripts/release_snapshot.py`
- `.github/scripts/release_pr_comment.py`
- `.github/workflows/release.yml`
- `.github/scripts/test-release-snapshot.sh`
- `README.md`
- `docs/specs/README.md`

### Out of scope

- 业务代码（`crates/**`, `web/**`）
- release label gate 与 release channel 定义
- GitHub Release 文案模板与 asset packing 策略

## 需求（Requirements）

### MUST

- `refs/notes/release-publications` 必须记录 `target_sha`、`pr_number`、`release_tag`、`release_channel`、`published_at`、`dockrev_digest`、`dockrev_supervisor_digest`。
- stable release 的 `latest` 判定必须只依赖“当前 `main` 一阶父链上是否已存在更晚 stable publication”，不得再被“更晚但尚未发布的 stable snapshot”压制。
- rc release 永远不得发布 `latest`，但必须回写源 PR 的 release-version issue comment。
- PR 发版评论必须固定定位到 snapshot 中的 `pr_number`，不得重新根据触发 run 的 head SHA 推断。
- PR 发版评论必须使用 issue/timeline comment，并以 `<!-- codex-release-version-comment -->` 做 bot-owned marker upsert。
- 若 marker 已被非 `github-actions[bot]` 评论占用，workflow 只能 warning 并跳过更新，不得覆盖外部评论。
- `Release` workflow 在镜像 push 成功后，必须先写入并推送 `release-publications` note，再创建/更新 GitHub Release。

### SHOULD

- older rerun / manual backfill 在不存在更晚 published stable note 时仍应带 `latest`，以修复漏发后的当前 stable 头部。
- 一旦更晚 stable publication 已存在，older rerun / backfill 应继续发布版本 tag 与 Release，但不得把 `latest` 回拨到旧版本。
- `dockrev` 与 `dockrev-supervisor` 应共用同一 published-stable 判定，避免两个镜像的 `latest` 漂移。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- `prepare/export` 读取 immutable snapshot 后，若启用 `--resolve-publication-tags`，则根据 `refs/notes/release-publications` 重新计算 `publish_latest` / `tags_csv` / `supervisor_tags_csv`。
- `publish` job 在两个镜像 push 成功后，读取 push 结果中的 digest，写入当前 `target_sha` 对应的 publication note，并 push `refs/notes/release-publications`。
- GitHub Release 成功后，workflow 调用 PR comment upsert 脚本，把本次实际发布的 `release_tag` 回写到 snapshot 绑定的 PR。
- rerun / manual backfill 重新执行时，只要当前 target 对应 release tag 允许幂等发布，publication ledger 就负责阻止旧 stable 重新夺回 `latest`。

### Edge cases / errors

- 若 publication note push 失败，workflow 必须失败，避免 GHCR 与 ledger 事实分裂。
- 若 GitHub Release 成功但 PR comment upsert 失败，workflow 必须失败并提示是 comment step 失败，便于重新运行补齐审计记录。
- 若 snapshot 中 `pr_number` 缺失，comment step 必须失败，因为该情况表示 snapshot 数据不完整。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `refs/notes/release-publications` | Git notes JSON payload | internal | New | None | CI maintainers | `release_snapshot.py`, `release.yml` | mutable publication ledger |
| `.github/scripts/release_snapshot.py export` | CLI output | internal | Modify | None | CI maintainers | `release.yml` | `latest` 改为按 published stable 判定 |
| `.github/scripts/release_pr_comment.py` | CLI | internal | New | None | CI maintainers | `release.yml` | PR timeline comment upsert |
| PR release-version issue comment | GitHub issue comment | external | New | None | CI maintainers | maintainers / reviewers | marker-based upsert |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given `0.35.8` 已有 stable snapshot 且 `0.35.9` 仅有 snapshot 尚未发布，When `0.35.8` export publication tags，Then `DOCKREV_TAGS_CSV` 与 `SUPERVISOR_TAGS_CSV` 仍包含 `:latest`。
- Given `0.35.9` 已写入 stable publication note，When rerun/backfill `0.35.8`，Then 仍发布 `0.35.8` version tag，但不再带 `:latest`。
- Given `channel:rc` target，When release 完成，Then 只发布 `*-rc.<sha7>`，GitHub Release 为 prerelease，且 PR comment 正文展示 RC tag。
- Given PR 上不存在 marker comment，When release comment step 运行，Then 创建单条 marker issue comment。
- Given PR 上已存在 `github-actions[bot]` 的 marker comment，When release comment step 再次运行，Then 原评论被更新而不是重复创建。
- Given PR 上 marker 被外部用户占用，When release comment step 运行，Then workflow 输出 warning 并跳过更新，不覆盖该评论。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bash ./.github/scripts/test-release-snapshot.sh`
- `python3 -m py_compile .github/scripts/release_snapshot.py .github/scripts/release_pr_comment.py`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml")'`

### UI / Storybook (if applicable)

- None

## 文档更新（Docs to Update）

- `README.md`: 更新 `latest` 定义与 PR 发版评论行为。
- `docs/specs/README.md`: 新增本 spec 索引，并注明它是当前 release publication 语义来源。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增 publication ledger schema / CLI，并将 `latest` 判定切换到已发布 stable。
- [x] M2: `Release` workflow 在镜像 push 后写入 publication ledger，并给 publish job 增加 `issues: write`。
- [x] M3: 新增 PR release-version issue comment upsert 脚本并接入 workflow。
- [x] M4: 回归测试、README 与 specs index 同步完成。
- [ ] M5: 快车道收敛到 latest PR merge-ready。

## 方案概述（Approach, high-level）

- 保留 immutable snapshot 作为“发布意图与版本号”真相源，再增加 mutable publication ledger 记录“哪些版本已真正发布出去”。
- `latest` 不再通过“看后面是否还有 stable snapshot”推断，而是通过“看当前主线是否已有更晚 stable publication”判定。
- PR 发版评论直接绑定 snapshot 里的 `pr_number`，把 release 结果回写到同一条 issue comment，形成稳定审计入口。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：镜像 digest 记录依赖 build-push-action 输出；workflow 需要明确从 step outputs 读取而不是再做 registry 查询。
- 风险：publication ledger 属于 mutable notes ref，rerun 并发下仍需保证 last-write-wins 的幂等性和 note push 失败时的显式报错。
- 假设：release snapshot 已稳定记录 `pr_number`，足以作为 comment upsert 的唯一定位依据。

## 变更记录（Change log）

- 2026-03-28: 创建规格，冻结 `latest = newest published stable` 与 PR release-version issue comment upsert 契约，并声明 supersede `#48mh8` 中关于 comment 的旧 non-goal。
- 2026-03-28: 完成本地实现与验证：`release_snapshot.py` 新增 `refs/notes/release-publications` / `record-publication`，`release.yml` 改为在镜像 push 后写 publication ledger 并于 GitHub Release 成功后 upsert PR issue comment，回归自测覆盖 `0.35.8/0.35.9` 队列与 comment create/update/foreign-marker/rc 场景。

## 参考（References）

- `~/.style-playbook-skills/skills/style-playbook/references/tags/pr-label-release.md`
- `docs/specs/48mh8-release-snapshot-queue-alignment/SPEC.md`
- `docs/specs/mzqkx-release-channel-selection/SPEC.md`
