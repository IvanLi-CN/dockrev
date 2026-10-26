# Dockrev

本文定义自检与操作可用性相关的领域术语。其他既有领域术语见 [CONTEXT.md](./CONTEXT.md)。

## 自检与操作可用性

**服务端自检（server self-check）**：由 Dockrev 服务端负责的运行环境和功能配置诊断，其生命周期独立于网页访问。
_Avoid_: 网页启动检查、浏览器部署检查

**启动自检（startup self-check）**：一次服务端启动后主动进行的首轮服务端自检。
_Avoid_: 首次打开网页检查、每标签页检查

**后台自检（background self-check）**：服务端在启动自检之后自主进行的复查，其进行本身不表示用户需要采取行动。
_Avoid_: 网页后台刷新、前台恢复检查

**自检结论（self-check finding）**：已完成诊断所确认的功能可用性事实。检查尚未完成或报告暂时无法读取不构成故障结论。
_Avoid_: 等待结果、报告请求状态

**核心故障（core capability failure）**：影响 Dockrev 核心管理能力的确定故障，是启动阶段自动展示自检页的依据。
_Avoid_: 任意 FAIL、可选功能配置问题

**功能故障（feature failure）**：影响某项已启用功能的确定故障，其影响范围取决于该功能与具体操作的依赖关系。
_Avoid_: 全站不可用、全站门禁

**自检页（self-check page）**：展示自检结论、影响和修复建议的诊断入口，保留继续浏览和前往修复入口的能力。
_Avoid_: 启动欢迎页、全站锁定页

**操作前置校验（operation prerequisite validation）**：判断一次具体操作所依赖条件是否满足的校验；其判断对象是该操作，不是用户能否浏览整个应用。
_Avoid_: 网页放行、自检报告放行

## Stack 管理

**归档 Stack**：用户主动隐藏的 Stack。Dockrev 仍保留其管理记录和恢复入口；归档不改变 Discovery 状态、Docker 资源归属或 Docker 资源回收资格。
_Avoid_: 删除 Stack、停止 Stack

**缺失 Stack**：Discovery 确认该 Stack 保存的所有 Compose 文件都以 `ENOENT` 缺失的状态。它与用户归档相互独立，并且是唯一允许执行永久删除记录的状态。
_Avoid_: 停止 Stack、无效 Stack

**永久删除缺失 Stack 记录**：永久移除缺失 Stack 在 Dockrev 中的 Stack、服务、历史、任务关联、备份记录和备份文件；该术语不包含删除宿主机 Compose 文件或 Docker 运行资源。
_Avoid_: 归档恢复、Docker 资源回收、重新部署

**归档恢复**：解除用户归档状态，使仍保留的管理记录恢复为未归档可见状态；它不改变 `missing` 状态，不代表重新部署，也不能恢复已经永久删除的原记录。
_Avoid_: 重新部署、重建 Stack

**Stack 删除审计记录**：证明一次永久删除记录操作发生过的最小操作记录，包含目标、操作者、时间和结果，但不保留已删除 Stack 的业务数据，也不提供恢复能力。
_Avoid_: 归档恢复记录、Stack 历史、备份内容

**Docker 资源回收**：按照 Docker 未使用资源规则处理容器、镜像、卷、网络和构建缓存的独立操作；它与永久删除缺失 Stack 记录分开，且不包含宿主机 Compose 文件或 Dockrev 记录的删除。
_Avoid_: 永久删除缺失 Stack 记录、删除 Compose 文件

**资源归属**：Dockrev 将 Docker 资源关联到 Stack 或 Service 的管理关系，用于清理结果分组和安全判断；它独立于 Stack 是否对用户可见以及容器当前是否运行。
_Avoid_: 资源存在、运行状态、用户归档状态
