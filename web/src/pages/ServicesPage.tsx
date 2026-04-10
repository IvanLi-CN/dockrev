import { type MouseEvent, type ReactNode } from 'react'
import {
  restoreService,
  restoreStack,
  type Service,
} from '../api'
import { navigate } from '../routes'
import { buildUpdateServiceTarget, buildUpdateServiceTargets } from '../updateTargets'
import { ArrowRightIcon, Button, Input, Mono, Pill, StatusRemark } from '../ui'
import { ImageLinkIcons, splitImageNameForDisplay, splitImageRef } from '../imageLinks'
import { isDockrevImageRef } from '../runtimeConfig'
import { isSemverDowngradeAnomaly, type RowStatus } from '../updateStatus'
import { UpdateCandidateFilters } from '../components/UpdateCandidateFilters'
import { DOCKREV_AGGREGATE_GUARD_HINT, resolveAggregateUpdateActionState } from '../aggregateUpdateGuard'
import { VersionTagsPopover } from '../components/VersionTagsPopover'
import { CurrentVersionPopover } from '../components/CurrentVersionPopover'
import { AggregateUpdatePreviewList } from '../components/AggregateUpdatePreviewList'
import { ConfirmServiceVersionCell } from '../components/ConfirmServiceVersionCell'
import {
  formatCandidateTagDisplay,
  formatCurrentTagDisplay as formatTagDisplay,
  isStrictSemverTag,
} from '../versionDisplay'
import { resolveUpdateActionTargetKey } from '../updateActionTracking'
import { useServicesPageState } from './useServicesPageState'

function formatShort(ts: string) {
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function formatGroupSummary(services: number, counts: Record<Exclude<RowStatus, 'ok'>, number>) {
  const parts: string[] = [`${services} services`]
  if (counts.updatable > 0) parts.push(`${counts.updatable} 可更新`)
  if (counts.hint > 0) parts.push(`${counts.hint} 需确认`)
  if (counts.archMismatch > 0) parts.push(`${counts.archMismatch} 架构不匹配`)
  if (counts.blocked > 0) parts.push(`${counts.blocked} 被阻止`)
  return parts.join(' · ')
}

function isDockrevService(svc: Service): boolean {
  return isDockrevImageRef(svc.image.ref)
}

function shouldPrefetchFloatingCandidate(
  candidateTag: string | null | undefined,
  candidateResolvedTag: string | null | undefined,
  candidateDigest: string | null | undefined,
): boolean {
  const raw = (candidateTag ?? '').trim()
  if (raw === '-') return false
  if (!raw || isStrictSemverTag(raw)) return false
  if (isStrictSemverTag(candidateResolvedTag)) return false
  return (candidateDigest ?? '').trim().length > 0
}

function StackIcon(props: { variant: 'collapsed' | 'expanded' }) {
  return (
    <svg className="stackIcon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {props.variant === 'expanded' ? (
        <path d="m5 19l2.757-7.351A1 1 0 0 1 8.693 11H21a1 1 0 0 1 .986 1.164l-.996 5.211A2 2 0 0 1 19.026 19za2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h4l3 3h7a2 2 0 0 1 2 2v2" />
      ) : (
        <path d="M5 4h4l3 3h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2" />
      )}
    </svg>
  )
}

function GroupGuide() {
  return <div className="groupGuide" aria-hidden="true" />
}

export function ServicesPage(props: {
  onLastScanHint: (lastScan?: string) => void
  onTopActions: (node: ReactNode) => void
}) {
  const {
    archivedDetails,
    archivedServices,
    archivedStacks,
    busy,
    collapsed,
    error,
    filter,
    filterSummary,
    getActiveJobByTarget,
    groups,
    isTargetBusy,
    isTargetSubmitting,
    noticeCheckJobId,
    noticeJobId,
    patchServiceInStackDetails,
    requestRefresh,
    search,
    selfUpgradeUrl,
    setBusy,
    setCollapsed,
    setError,
    setFilter,
    setSearch,
    supervisor,
    totals,
    triggerApply,
  } = useServicesPageState(props)



  return (
	    <div className="page">
	      <div className="card">
	        <div className="sectionRow">
	          <div className="title">服务</div>
	          <div style={{ marginLeft: 'auto', display: 'flex', gap: 10, alignItems: 'center' }}>
            <Input
              className="input"
              onChange={(e) => setSearch(e.target.value)}
              placeholder="搜索 service / image / stack"
              value={search}
            />
	            <div className="muted">
	              {totals.filtered}/{totals.total}
	            </div>
	          </div>
	        </div>

	        <div style={{ marginTop: 12 }}>
	          <UpdateCandidateFilters value={filter} onChange={setFilter} total={filterSummary.total} counts={filterSummary.counts} />
	        </div>

	        <div className="table" style={{ marginTop: 12 }}>
	          <div className="tableHeader">
	            <div>Service</div>
	            <div>Image</div>
	            <div>Versions</div>
            <div>状态 / 备注</div>
            <div>操作</div>
          </div>

	          {groups.map((g) => {
	            const isCollapsed = collapsed[g.stackId] ?? false
	            const groupSummary = formatGroupSummary(g.totalServices, g.aggregatePartition.counts)
              const stackApply = resolveAggregateUpdateActionState(g.aggregatePartition)
              const stackApplyActionKey = resolveUpdateActionTargetKey('stack', g.stackId, null)
              const stackApplyActiveJob = stackApplyActionKey ? getActiveJobByTarget(stackApplyActionKey) : null
              const stackApplySubmitting = stackApplyActionKey ? isTargetSubmitting(stackApplyActionKey) : false
	            return (
	              <div key={g.stackId} className={isCollapsed ? 'tableGroup' : 'tableGroup tableGroupExpanded'}>
	                {!isCollapsed ? <GroupGuide /> : null}
                <div
                  className="groupHead"
                  onClick={() => setCollapsed((prev) => ({ ...prev, [g.stackId]: !isCollapsed }))}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault()
                      setCollapsed((prev) => ({ ...prev, [g.stackId]: !isCollapsed }))
                    }
                  }}
                >
                  <div className="cellService cellServiceGroup">
                    <StackIcon variant={isCollapsed ? 'collapsed' : 'expanded'} />
                    <div className="groupTitle">{g.stackName}</div>
                  </div>
                  <div className="groupMeta">{groupSummary}</div>
                  <div />
                  <div />
                  <div
                    className="actionCell"
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={(e) => e.stopPropagation()}
                  >
                    <Button
                      variant="ghost"
                      disabled={
                        stackApplyActiveJob
                          ? false
                          : !stackApply.enabled || busy || stackApplySubmitting
                      }
                      loading={stackApplyActionKey ? isTargetBusy(stackApplyActionKey) : false}
                      loadingClickable={Boolean(stackApplyActiveJob)}
                      title={stackApplyActiveJob ? '任务进行中，点击查看任务详情' : (stackApply.title ?? undefined)}
                      hint={stackApplyActiveJob ? '任务进行中，点击查看任务详情' : (!stackApply.enabled ? (stackApply.hint ?? undefined) : undefined)}
                      onClick={() => {
                            if (stackApplyActiveJob) {
                              navigate({ name: 'job', jobId: stackApplyActiveJob.jobId })
                              return
                            }
		                        const previewItems = [
                          ...g.aggregatePartition.actionable.map((item) => ({
                            ...item,
                            displayName: item.svc.name,
                            stackId: g.stackId,
                          })),
                          ...g.aggregatePartition.guardedDockrevPreview.map((item) => ({
                            ...item,
                            displayName: item.svc.name,
                            stackId: g.stackId,
                          })),
                        ]
                        const anomalyCount = previewItems.filter((item) =>
                          isSemverDowngradeAnomaly(item.svc),
                        ).length
                        const totalCandidates = g.aggregatePartition.actionable.length
                        const body = (
                          <>
                            <div className="modalKvGrid">
                              <div className="modalKvLabel">范围</div>
                              <div className="modalKvValue">
                                <Mono>stack</Mono>
                              </div>
                              <div className="modalKvLabel">目标</div>
                              <div className="modalKvValue">
                                <Mono>{g.stackName}</Mono>
                              </div>
                              <div className="modalKvLabel">候选服务</div>
                              <div className="modalKvValue">{totalCandidates} 个（可更新/需确认）</div>
                              <div className="modalKvLabel">其中</div>
                              <div className="modalKvValue">
                                可更新 {g.aggregatePartition.counts.updatable} · 需确认 {g.aggregatePartition.counts.hint}
                              </div>
                              <div className="modalKvLabel">将跳过</div>
                              <div className="modalKvValue">
                                架构不匹配 {g.aggregatePartition.counts.archMismatch} · 被阻止 {g.aggregatePartition.counts.blocked}
                              </div>
                            </div>
                            {anomalyCount > 0 ? (
                              <div className="muted" style={{ marginTop: 10 }}>
                                ⚠ 检测到 {anomalyCount} 个版本异常（候选低于当前）；手动确认后仍可继续更新。
                              </div>
                            ) : null}
	                            <div className="modalDivider" />
	                            <div className="modalLead">将更新的服务（预览）</div>
	                            <AggregateUpdatePreviewList
	                              items={previewItems}
	                              dockrevGuardHint={DOCKREV_AGGREGATE_GUARD_HINT}
                                onServiceResolvedTags={(update) => {
                                  const stackId = (update.stackId ?? '').trim() || g.stackId
                                  patchServiceInStackDetails(stackId, update.serviceId, (prev) => ({
                                    ...prev,
                                    image: {
                                      ...prev.image,
                                      resolvedTag: update.resolvedTag,
                                      resolvedTags: update.resolvedTags,
                                    },
                                  }))
                                }}
                                onServiceCandidateResolvedTag={(update) => {
                                  const stackId = (update.stackId ?? '').trim() || g.stackId
                                  patchServiceInStackDetails(stackId, update.serviceId, (prev) => ({
                                    ...prev,
                                    candidate: prev.candidate
                                      ? {
                                          ...prev.candidate,
                                          resolvedTag: update.resolvedTag,
                                        }
                                      : prev.candidate,
                                  }))
                                }}
	                            />
	                            <div className="modalDivider" />
	                          </>
	                        )
                                                void triggerApply({
	                          scope: 'stack',
	                          stackId: g.stackId,
	                          targetLabel: `stack:${g.stackName}`,
	                          buildRequest: async () => ({
	                            scope: 'stack',
	                            stackId: g.stackId,
	                            targets: await buildUpdateServiceTargets(
	                              g.aggregatePartition.actionable.map((item) => item.svc),
	                            ),
	                            mode: 'apply',
	                            allowArchMismatch: false,
	                            backupMode: 'inherit',
	                          }),
	                          confirmBody: body,
	                          confirmTitle: '确认更新此 stack？',
		                        })
		                      }}
	                    >
                        {stackApplyActiveJob?.status === 'queued'
                          ? '排队中…'
                          : stackApplyActiveJob
                            ? '更新中…'
                            : stackApplySubmitting
                              ? '提交中…'
                              : '更新此 stack'}
	                    </Button>
                  </div>
                </div>

                {!isCollapsed
                  ? g.services.map(({ svc, status }) => {
                      const isDockrev = isDockrevService(svc)
                      const currentDisplayTag = formatTagDisplay(
                        svc.image.tag,
                        svc.image.resolvedTag,
                        svc.versionInference?.status,
                      )
                      const inferencePending = svc.versionInference?.status === 'pending'
                      const rawTagTrim = (svc.image.tag ?? '').trim()
                      const showRawTag = Boolean(rawTagTrim && rawTagTrim !== currentDisplayTag)
                      const candidateRawTag = svc.candidate?.tag && svc.candidate.tag !== '-' ? svc.candidate.tag : null
                      const candidateDisplayTag = candidateRawTag
                        ? formatCandidateTagDisplay(
                            candidateRawTag,
                            svc.candidate?.resolvedTag ?? null,
                            svc.versionInference?.status,
                          )
                        : null
                      const showCandidate = Boolean(candidateDisplayTag && candidateDisplayTag !== currentDisplayTag)
                      const candidatePrefetchOnMount =
                        candidateRawTag && candidateDisplayTag
                          ? shouldPrefetchFloatingCandidate(
                              candidateRawTag,
                              svc.candidate?.resolvedTag ?? null,
                              svc.candidate?.digest ?? null,
                            )
                          : false
                      const arrowPulse = inferencePending
	                      const svcApply =
	                        status === 'updatable'
	                          ? { enabled: true, title: null as string | null }
	                          : status === 'hint'
	                            ? { enabled: true, title: '需确认候选；将由服务端计算是否实际变更' }
	                            : status === 'ok'
	                              ? { enabled: false, title: '无候选版本' }
	                              : status === 'archMismatch'
	                                ? { enabled: false, title: '架构不匹配（仅提示，不允许更新）' }
	                                : { enabled: false, title: svc.ignore?.reason ?? '被阻止' }
                      const svcApplyActionKey = resolveUpdateActionTargetKey('service', null, svc.id)
                      const svcApplyActiveJob = svcApplyActionKey ? getActiveJobByTarget(svcApplyActionKey) : null
                      const svcApplySubmitting = svcApplyActionKey ? isTargetSubmitting(svcApplyActionKey) : false
                      return (
                        <div
                          key={svc.id}
                          className="rowLine"
                          onClick={(e) => {
                            const t = e.target as unknown
                            const el =
                              t instanceof Element
                                ? t
                                : t && (t as { parentElement?: unknown }).parentElement instanceof Element
                                  ? (t as { parentElement: Element }).parentElement
                                  : null
                            if (el?.closest('button, a, input, select, textarea')) return
                            navigate({ name: 'service', stackId: g.stackId, serviceId: svc.id })
                          }}
                          role="button"
                          tabIndex={0}
                          onKeyDown={(e) => {
                            const t = e.target as unknown
                            const el =
                              t instanceof Element
                                ? t
                                : t && (t as { parentElement?: unknown }).parentElement instanceof Element
                                  ? (t as { parentElement: Element }).parentElement
                                  : null
                            if (el?.closest('button, a, input, select, textarea')) return
                            if (e.key === 'Enter' || e.key === ' ') {
                              e.preventDefault()
                              navigate({ name: 'service', stackId: g.stackId, serviceId: svc.id })
                            }
                          }}
                        >
	                          <div className="cellService">
	                            <span className="svcBullet" aria-hidden="true" />
	                            <span className="svcName">{svc.name}</span>
	                          </div>
                          {(() => {
                            const img = splitImageRef(svc.image.ref)
                            const dn = splitImageNameForDisplay(img.name, svc.image.tag)
                            const stopRowLink = (event: MouseEvent<HTMLAnchorElement>) => {
                              event.stopPropagation()
                            }
                            return (
                              <div className="cellTwoLine">
                                <div className="mono monoPrimary monoSplit imageLinkRow" title={dn.suffix ? `${dn.base}${dn.suffix}` : dn.base}>
                                  <span className="monoSplitBase">{dn.base}</span>
                                  <ImageLinkIcons imageRef={svc.image.ref} onClick={stopRowLink} repoUrl={svc.settings.repoUrl} />
                                </div>
                                <div className="mono monoSecondary">{img.registry}</div>
                              </div>
                            )
                          })()}
	                          <div className="cellTwoLine">
                              <div className="versionLine">
                                <CurrentVersionPopover
                                  serviceId={svc.id}
                                  displayTag={currentDisplayTag}
                                  imageTag={svc.image.tag}
                                  imageDigest={svc.image.digest ?? null}
                                  resolvedTag={svc.image.resolvedTag}
                                  resolvedTags={svc.image.resolvedTags}
                                  onLocalResolvedTags={(update) => {
                                    patchServiceInStackDetails(
                                      g.stackId,
                                      svc.id,
                                      (prev) => ({
                                        ...prev,
                                        image: {
                                          ...prev.image,
                                          resolvedTag: update.resolvedTag,
                                          resolvedTags: update.resolvedTags,
                                        },
                                      }),
                                    )
                                  }}
                                  inferenceLoading={inferencePending}
                                />
                                {showCandidate ? (
                                  <>
                                    <span className={arrowPulse ? 'inlineIconLoading' : 'inlineIconMuted'}>
                                      <ArrowRightIcon className="inlineIcon" />
                                    </span>
                                    <VersionTagsPopover
                                      serviceId={svc.id}
                                      candidateTag={candidateRawTag}
                                      candidateDigest={svc.candidate?.digest ?? null}
                                      prefetchOnMount={candidatePrefetchOnMount}
                                      onLocalResolvedTag={(resolvedTag) => {
                                        patchServiceInStackDetails(
                                          g.stackId,
                                          svc.id,
                                          (prev) => ({
                                            ...prev,
                                            candidate: prev.candidate
                                              ? {
                                                  ...prev.candidate,
                                                  resolvedTag,
                                                }
                                              : prev.candidate,
                                          }),
                                        )
                                      }}
                                    >
                                      {candidateDisplayTag}
                                    </VersionTagsPopover>
                                  </>
                                ) : null}
                              </div>
	                            {showRawTag ? (
                                <div>
                                  <CurrentVersionPopover
                                    serviceId={svc.id}
                                    displayTag={svc.image.tag}
                                    imageTag={svc.image.tag}
                                    imageDigest={svc.image.digest ?? null}
                                    resolvedTag={svc.image.resolvedTag}
                                    resolvedTags={svc.image.resolvedTags}
                                    onLocalResolvedTags={(update) => {
                                      patchServiceInStackDetails(
                                        g.stackId,
                                        svc.id,
                                        (prev) => ({
                                          ...prev,
                                          image: {
                                            ...prev.image,
                                            resolvedTag: update.resolvedTag,
                                            resolvedTags: update.resolvedTags,
                                          },
                                        }),
                                      )
                                    }}
                                    preferSource="rawTag"
                                    triggerClassName="versionTagsTrigger mono monoSecondary"
                                  >
                                    {svc.image.tag}
                                  </CurrentVersionPopover>
                                </div>
	                            ) : null}
	                          </div>
                          <StatusRemark service={svc} status={status} />
	                          <div
	                            className="actionCell"
	                            onClick={(e) => e.stopPropagation()}
	                            onKeyDown={(e) => e.stopPropagation()}
	                          >
                            {isDockrev ? (
                              <div className="actionStack">
                                <Button
                                  variant="ghost"
                                  disabled={busy || supervisor.state.status !== 'ok'}
                                  title={
                                    supervisor.state.status === 'offline'
                                      ? `自我升级不可用（supervisor offline） · ${supervisor.state.errorAt} · ${supervisor.state.error}`
                                      : supervisor.state.status === 'checking'
                                        ? '检查 supervisor 中…'
                                        : undefined
                                  }
                                  onClick={() => {
                                    window.location.href = selfUpgradeUrl
                                  }}
                                >
                                  升级 Dockrev
                                </Button>
                                {supervisor.state.status !== 'ok' ? (
                                  <Button
                                    variant="ghost"
                                    disabled={busy || supervisor.state.status === 'checking'}
                                    onClick={() => {
                                      void supervisor.check()
                                    }}
                                  >
                                    重试
                                  </Button>
                                ) : null}
                                {supervisor.state.status === 'offline' ? (
                                  <div className="muted">
                                    supervisor offline · {supervisor.state.errorAt} · <Mono>{supervisor.state.error}</Mono>
                                  </div>
                                ) : null}
                              </div>
                            ) : (
                              <Button
                                variant="ghost"
                                disabled={
                                  svcApplyActiveJob
                                    ? false
                                    : !svcApply.enabled || busy || svcApplySubmitting
                                }
                                loading={svcApplyActionKey ? isTargetBusy(svcApplyActionKey) : false}
                                loadingClickable={Boolean(svcApplyActiveJob)}
                                title={svcApplyActiveJob ? '任务进行中，点击查看任务详情' : (svcApply.title ?? undefined)}
                                hint={svcApplyActiveJob ? '任务进行中，点击查看任务详情' : undefined}
                                onClick={() => {
                                          if (svcApplyActiveJob) {
                                            navigate({ name: 'job', jobId: svcApplyActiveJob.jobId })
                                            return
                                          }
		                                  const body = (
		                                    <>
	                                      <div className="modalLead">将对该服务执行更新（apply）。</div>
	                                      <div className="modalKvGrid">
	                                        <div className="modalKvLabel">范围</div>
	                                        <div className="modalKvValue">
	                                          <Mono>service</Mono>
	                                        </div>
	                                        <div className="modalKvLabel">目标</div>
	                                        <div className="modalKvValue">
	                                          <Mono>{`${g.stackName}/${svc.name}`}</Mono>
		                                        </div>
		                                        <div className="modalKvLabel">镜像</div>
		                                        <div className="modalKvValue">
		                                          {(() => {
		                                        const img = splitImageRef(svc.image.ref)
		                                        const dn = splitImageNameForDisplay(img.name, svc.image.tag)
		                                          return (
		                                            <div className="cellTwoLine">
		                                            <div className="mono monoPrimary monoSplit imageLinkRow" title={dn.suffix ? `${dn.base}${dn.suffix}` : dn.base}>
		                                              <span className="monoSplitBase">{dn.base}</span>
		                                              <ImageLinkIcons imageRef={svc.image.ref} repoUrl={svc.settings.repoUrl} />
		                                            </div>
		                                            <div className="mono monoSecondary">{img.registry}</div>
		                                          </div>
		                                        )
		                                      })()}
		                                        </div>
		                                        <div className="modalKvLabel">目标版本</div>
		                                        <div className="modalKvValue">
                                            <ConfirmServiceVersionCell
                                              serviceId={svc.id}
                                              imageTag={svc.image.tag}
                                              imageDigest={svc.image.digest ?? null}
                                              resolvedTag={svc.image.resolvedTag}
                                              resolvedTags={svc.image.resolvedTags}
                                              inferenceStatus={svc.versionInference?.status}
                                              candidateTag={candidateRawTag}
                                              candidateDigest={svc.candidate?.digest ?? null}
                                              candidateResolvedTag={svc.candidate?.resolvedTag}
                                              prefetchOnMount={candidatePrefetchOnMount}
                                              onHostResolvedTags={(update) => {
                                                patchServiceInStackDetails(g.stackId, svc.id, (prev) => ({
                                                  ...prev,
                                                  image: {
                                                    ...prev.image,
                                                    resolvedTag: update.resolvedTag,
                                                    resolvedTags: update.resolvedTags,
                                                  },
                                                }))
                                              }}
                                              onHostCandidateResolvedTag={(resolvedTag) => {
                                                patchServiceInStackDetails(g.stackId, svc.id, (prev) => ({
                                                  ...prev,
                                                  candidate: prev.candidate
                                                    ? {
                                                        ...prev.candidate,
                                                        resolvedTag,
                                                      }
                                                    : prev.candidate,
                                                }))
                                              }}
                                            />
	                                        </div>
		                                        <div className="modalKvLabel">状态</div>
		                                        <div className="modalKvValue">
		                                          <Mono>{status}</Mono>
		                                        </div>
		                                      </div>
		                                      <div className="modalDivider" />
		                                    </>
	                                  )
			                                  void triggerApply({
			                                    scope: 'service',
			                                    stackId: g.stackId,
			                                    serviceId: svc.id,
			                                    targetLabel: `service:${g.stackName}/${svc.name}`,
			                                    buildRequest: async () => ({
			                                      scope: 'service',
			                                      stackId: g.stackId,
			                                      ...(await buildUpdateServiceTarget(svc)),
			                                      mode: 'apply',
			                                      allowArchMismatch: false,
			                                      backupMode: 'inherit',
			                                    }),
			                                    confirmBody: body,
			                                    confirmTitle: `确认更新服务 ${svc.name}？`,
			                                  })
		                                }}
	                              >
                                  {svcApplyActiveJob?.status === 'queued'
                                    ? '排队中…'
                                    : svcApplyActiveJob
                                      ? '更新中…'
                                      : svcApplySubmitting
                                        ? '提交中…'
                                        : '执行更新'}
	                              </Button>
	                            )}
	                          </div>
                        </div>
                      )
                    })
                  : null}
              </div>
            )
          })}

          {groups.length === 0 ? <div className="muted">无匹配结果</div> : null}
        </div>
      </div>

      <div className="card">
        <div className="sectionRow">
          <div className="title">已归档</div>
        </div>
        {archivedStacks.length === 0 && archivedServices.length === 0 ? <div className="muted">暂无归档对象</div> : null}

        {archivedStacks.length > 0 ? (
          <div style={{ marginTop: 10 }}>
            <div className="muted" style={{ marginBottom: 8 }}>
              已归档 stacks（按 stack 成组展示）
            </div>
            <div className="svcGrid">
              {archivedStacks.map((st) => {
                const d = archivedDetails[st.id]
                const title = d ? d.name : st.name
                return (
                  <div key={st.id} className="svcCard" style={{ cursor: 'default' }}>
                    <div className="svcCardTop">
                      <div className="svcCardName">{title}</div>
                      <Pill tone="muted">archived</Pill>
                    </div>
                    <div className="svcCardMeta">
                      <div className="muted">
                        id <Mono>{st.id}</Mono>
                      </div>
                      <div className="muted">
                        services <Mono>{st.services}</Mono> · archived services <Mono>{st.archivedServices ?? 0}</Mono> · updates{' '}
                        <Mono>{st.updates}</Mono>
                      </div>
                      <div className="muted">
                        last scan <Mono>{formatShort(st.lastCheckAt)}</Mono>
                      </div>
                      <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
                        <Button
                          variant="primary"
                          disabled={busy}
                          onClick={() => {
                            void (async () => {
                              setBusy(true)
                              setError(null)
                              try {
                                await restoreStack(st.id)
                                await requestRefresh()
                              } catch (e: unknown) {
                                setError(e instanceof Error ? e.message : String(e))
                              } finally {
                                setBusy(false)
                              }
                            })()
                          }}
                        >
                          恢复 stack
                        </Button>
                      </div>
                    </div>
                  </div>
                )
              })}
            </div>
          </div>
        ) : null}

      {archivedServices.length > 0 ? (
          <div style={{ marginTop: 16 }}>
            <div className="muted" style={{ marginBottom: 8 }}>
              已归档 services（按所属 stack 聚合）
            </div>
            <div className="svcGrid">
              {archivedServices.map((x) => (
                <div key={x.svc.id} className="svcCard" style={{ cursor: 'default' }}>
                  <div className="svcCardTop">
                    <div className="svcCardName">{x.svc.name}</div>
                    <Pill tone="muted">archived</Pill>
                  </div>
                  <div className="svcCardMeta">
                    <div className="muted">
                      stack <Mono>{x.stackName}</Mono>
                    </div>
	                    {(() => {
	                      const img = splitImageRef(x.svc.image.ref)
	                      const dn = splitImageNameForDisplay(img.name, x.svc.image.tag)
	                      return (
	                        <div className="muted">
	                          image{' '}
	                          <span className="mono" title={dn.suffix ? `${dn.base}${dn.suffix}` : dn.base}>
	                            {dn.base}
	                          </span>{' '}
	                          · registry <Mono>{img.registry}</Mono> · current{' '}
	                          <Mono>
                            {formatTagDisplay(
                              x.svc.image.tag,
                              x.svc.image.resolvedTag,
                              x.svc.versionInference?.status,
                            )}
                          </Mono>
	                        </div>
	                      )
	                    })()}
                    <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
                      <Button
                        variant="primary"
                        disabled={busy}
                        onClick={() => {
                          void (async () => {
                            setBusy(true)
                            setError(null)
                            try {
                              await restoreService(x.svc.id)
                              await requestRefresh()
                            } catch (e: unknown) {
                              setError(e instanceof Error ? e.message : String(e))
                            } finally {
                              setBusy(false)
                            }
                          })()
                        }}
                      >
                        恢复 service
                      </Button>
                      <Button
                        variant="ghost"
                        disabled={busy}
                        onClick={() => navigate({ name: 'service', stackId: x.stackId, serviceId: x.svc.id })}
                      >
                        打开详情
                      </Button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ) : null}
      </div>

      {error ? <div className="error">{error}</div> : null}
      {noticeJobId ? (
        <div className="success">
          已创建更新任务 <Mono>{noticeJobId}</Mono> ·{' '}
          <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'queue' })}>
            查看队列
          </Button>
        </div>
      ) : null}
      {noticeCheckJobId ? (
        <div className="success">
          扫描任务 <Mono>{noticeCheckJobId}</Mono> ·{' '}
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => navigate({ name: 'job', jobId: noticeCheckJobId })}
          >
            查看任务
          </Button>
        </div>
      ) : null}
      {busy ? <div className="muted">处理中…</div> : null}
    </div>
  )
}
