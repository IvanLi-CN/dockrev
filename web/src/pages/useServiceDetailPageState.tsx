import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ApiError, archiveService, createIgnore, getServiceRollbackTarget, getServiceSettings, getStack, getStackSettings, listIgnores, newJobEventsSource, restoreService, triggerRuntimeScan, triggerServiceRollback, triggerUpdate, type IgnoreRule, type Service, type ServiceRollbackTargetResponse, type ServiceSettings, type StackDetail, type StackSettings } from '../api'
import { readUpdateGuardBlockedReason } from '../aggregateUpdateGuard'
import { CurrentVersionPopover } from '../components/CurrentVersionPopover'
import { normalizeDigest } from '../components/digest'
import { ServiceUpdateConfirmDetails } from '../components/ServiceUpdateConfirmDetails'
import { VersionTagsPopover } from '../components/VersionTagsPopover'
import { useConfirm } from '../confirm'
import { DIGEST_SNAPSHOT_UPDATED_EVENT, type DigestSnapshotUpdatedDetail } from '../digestInferenceTracker'
import { normalizeExternalHttpUrl } from '../imageLinks'
import { imageRepoFromImageRef } from '../imageRepo'
import { errorMessage, formatMap, isDockrevService, normalizeMaybeDigest, rollbackTargetMatchesServiceDigest, rollbackUnavailableReasonLabel, rollbackVersionLabel, ROLLBACK_TARGET_REFRESH_HINT, scanHasFailures, scanIsComplete, shortDigest, shouldPrefetchFloatingCandidate, svcTone, useRollbackTargetInvariantWarning } from './serviceDetailUtils'
import { navigate } from '../routes'
import { selfUpgradeBaseUrl } from '../runtimeConfig'
import { Button, Mono } from '../ui'
import { UPDATE_JOB_SETTLED_EVENT, UPDATE_JOB_SETTLE_RETRY_MS, resolveUpdateActionTargetKey, useUpdateActionTracker, type UpdateJobSettledDetail } from '../updateActionTracking'
import { blockedReasonFor, isSemverDowngradeAnomaly, serviceRowStatus } from '../updateStatus'
import { buildUpdateServiceTarget } from '../updateTargets'
import { usePageResumeRefresh } from '../usePageResumeRefresh'
import { useSupervisorHealth } from '../useSupervisorHealth'
import { formatCandidateTagDisplay, formatCurrentTagDisplay as formatTagDisplay, inferResolvedTagsFromSnapshot, isStrictSemverTag } from '../versionDisplay'

export function useServiceDetailPageState(props: {
  stackId: string
  serviceId: string
  onLastScanHint: (lastScan?: string) => void
}) {
  const { stackId, serviceId, onLastScanHint } = props
  const confirm = useConfirm()
  const [stack, setStack] = useState<StackDetail | null>(null)
  const [service, setService] = useState<Service | null>(null)
  const [settings, setSettings] = useState<ServiceSettings | null>(null)
  const [stackSettings, setStackSettings] = useState<StackSettings | null>(null)
  const [rules, setRules] = useState<IgnoreRule[]>([])
  const [busy, setBusy] = useState(false)
  const [repoInferBusy, setRepoInferBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<{ jobId: string; kind: 'update' | 'rollback' } | null>(null)
  const [rollbackTarget, setRollbackTarget] = useState<ServiceRollbackTargetResponse | null>(null)
  const [rollbackActiveTarget, setRollbackActiveTarget] = useState<ServiceRollbackTargetResponse | null>(null)
  const { beginSubmitting, endSubmitting, trackJob, isTargetBusy, getActiveJobByTarget, isTargetSubmitting } =
    useUpdateActionTracker()
  const { state: supervisorState, check: checkSupervisor } = useSupervisorHealth()
  const supervisorErrorAt = supervisorState.status === 'offline' ? supervisorState.errorAt : undefined
  const supervisorError = supervisorState.status === 'offline' ? supervisorState.error : undefined
  const selfUpgradeUrl = useMemo(() => selfUpgradeBaseUrl(), [])
  const applyActionKey = useMemo(
    () => resolveUpdateActionTargetKey('service', stackId, serviceId),
    [serviceId, stackId],
  )
  const applyActionBusy = applyActionKey ? isTargetBusy(applyActionKey) : false
  const applyActiveJob = applyActionKey ? getActiveJobByTarget(applyActionKey) : null
  const applySubmitting = applyActionKey ? isTargetSubmitting(applyActionKey) : false
  const [rollbackTargetRefreshing, setRollbackTargetRefreshing] = useState(false)
  const rollbackStatusSource = rollbackTarget ?? rollbackActiveTarget
  const rollbackActiveJobId = (rollbackStatusSource?.activeJobId ?? '').trim() || null
  const rollbackActiveJobStatus = (rollbackStatusSource?.activeJobStatus ?? '').trim() || null
  const rollbackReason = rollbackTarget?.unavailableReason ?? rollbackActiveTarget?.unavailableReason ?? null
  const rollbackReasonLabel = rollbackUnavailableReasonLabel(rollbackReason)
  const rollbackTargetDigestRetryMs = 250
  const rollbackActionBusy = Boolean(rollbackActiveJobId)
  const rollbackHint = rollbackActiveJobId ? '任务进行中，点击查看任务详情' : rollbackTargetRefreshing ? ROLLBACK_TARGET_REFRESH_HINT : !rollbackTarget?.available ? rollbackReasonLabel : undefined
  const fullRefreshRequestIdRef = useRef(0)
  const latestAppliedFullRefreshRequestIdRef = useRef(0)
  const stackRefreshRequestIdRef = useRef(0)
  const latestAppliedStackRefreshRequestIdRef = useRef(0)

  const [newRuleKind, setNewRuleKind] = useState<'exact' | 'prefix' | 'regex' | 'semver'>('regex')
  const [newRuleValue, setNewRuleValue] = useState('.*')
  const [newRuleNote, setNewRuleNote] = useState('blocked via UI')

  const warnRollbackTargetDiscard = useCallback(
    (reason: string, requestId: number, svc: Service | null, target: ServiceRollbackTargetResponse | null, source: string) => {
      console.warn('[dockrev] discard rollback target response', {
        serviceId,
        requestId,
        latestAppliedStackRequestId: latestAppliedStackRefreshRequestIdRef.current,
        serviceDigest: normalizeMaybeDigest(svc?.image.digest),
        rollbackCurrentDigest: normalizeMaybeDigest(target?.currentDigest),
        reason,
        source,
      })
    },
    [serviceId],
  )

  const applyRollbackTargetSnapshot = useCallback((requestId: number, svc: Service | null, target: ServiceRollbackTargetResponse | null, source: string): 'applied' | 'outdated' | 'digest_mismatch' => {
    if (requestId < latestAppliedStackRefreshRequestIdRef.current) {
      warnRollbackTargetDiscard('outdated_request', requestId, svc, target, source)
      return 'outdated'
    }
    if (svc && !isDockrevService(svc) && target && !rollbackTargetMatchesServiceDigest(svc, target)) {
      warnRollbackTargetDiscard('current_digest_mismatch', requestId, svc, target, source)
      setRollbackActiveTarget((prev) => (target?.activeJobId ? target : prev)); setRollbackTarget(null); setRollbackTargetRefreshing(true)
      return 'digest_mismatch'
    }
    setRollbackTarget(target); setRollbackActiveTarget(target?.activeJobId ? target : null); setError((prev) => prev === '回滚信息刷新失败，请稍后重试' ? null : prev)
    setRollbackTargetRefreshing(false)
    return 'applied'
  }, [warnRollbackTargetDiscard])

  const settleRollbackTargetSnapshot = useCallback(async (requestId: number, svc: Service, target: ServiceRollbackTargetResponse | null, source: string) => {
    let nextTarget = target, nextSource = source, retries = 0
    for (;;) {
      const result = applyRollbackTargetSnapshot(requestId, svc, nextTarget, nextSource)
      if (result !== 'digest_mismatch') return result
      if (retries++ >= 5) {
        setRollbackTarget(null); setRollbackActiveTarget(null)
        setRollbackTargetRefreshing(false)
        setError('回滚信息刷新失败，请稍后重试')
        return 'digest_mismatch'
      }
      await new Promise<void>((resolve) => window.setTimeout(resolve, rollbackTargetDigestRetryMs))
      if (requestId < latestAppliedStackRefreshRequestIdRef.current) return 'outdated'
      nextTarget = await getServiceRollbackTarget(serviceId)
      nextSource = `${source}-digest-retry`
    }
  }, [applyRollbackTargetSnapshot, rollbackTargetDigestRetryMs, serviceId])

  const refresh = useCallback(async () => {
    const fullRefreshRequestId = ++fullRefreshRequestIdRef.current
    const stackRequestId = ++stackRefreshRequestIdRef.current
    let appliedFullRefreshRoot = false
    setError(null)
    onLastScanHint?.(undefined)
    try {
      const st = await getStack(stackId)
      const svc = st.services.find((s) => s.id === serviceId) ?? null
      if (stackRequestId >= latestAppliedStackRefreshRequestIdRef.current) {
        latestAppliedStackRefreshRequestIdRef.current = stackRequestId
        latestAppliedFullRefreshRequestIdRef.current = fullRefreshRequestId
        appliedFullRefreshRoot = true
        setStack(st)
        setService(svc)
        setRollbackTargetRefreshing(Boolean(svc && !isDockrevService(svc)))
        if (!svc || isDockrevService(svc)) { setRollbackTarget(null); setRollbackActiveTarget(null) }
      }

      const [settingsRes, rulesRes, rollbackRes] = await Promise.allSettled([
        getServiceSettings(serviceId),
        listIgnores(),
        svc && !isDockrevService(svc) ? getServiceRollbackTarget(serviceId) : Promise.resolve(null),
      ])
      const stackSettingsRes = await getStackSettings(stackId).then(
        (value) => ({ status: 'fulfilled' as const, value }),
        (reason: unknown) => ({ status: 'rejected' as const, reason }),
      )
      const errors: string[] = []

      if (settingsRes.status === 'rejected') errors.push(errorMessage(settingsRes.reason))
      if (stackSettingsRes.status === 'rejected') errors.push(errorMessage(stackSettingsRes.reason))
      if (rulesRes.status === 'rejected') errors.push(errorMessage(rulesRes.reason))
      if (rollbackRes.status === 'rejected') errors.push(errorMessage(rollbackRes.reason))

      if (fullRefreshRequestId < latestAppliedFullRefreshRequestIdRef.current) return

      if (settingsRes.status === 'fulfilled') setSettings(settingsRes.value)
      if (stackSettingsRes.status === 'fulfilled') setStackSettings(stackSettingsRes.value)
      if (rulesRes.status === 'fulfilled') {
        setRules(rulesRes.value.filter((r) => r.scope.serviceId === serviceId))
      }
      if (!svc || isDockrevService(svc)) {
        setRollbackTarget(null); setRollbackActiveTarget(null)
        setRollbackTargetRefreshing(false)
      } else if (rollbackRes.status === 'fulfilled') {
        await settleRollbackTargetSnapshot(stackRequestId, svc, rollbackRes.value, 'full-refresh')
      } else {
        setRollbackTarget(null); setRollbackActiveTarget(null); setRollbackTargetRefreshing(false)
      }
      if (errors.length > 0) throw new Error(errors.join(' · '))
    } catch (error: unknown) {
      if (!appliedFullRefreshRoot && stackRequestId < latestAppliedStackRefreshRequestIdRef.current) return
      if (appliedFullRefreshRoot && fullRefreshRequestId < latestAppliedFullRefreshRequestIdRef.current) return
      setRollbackTargetRefreshing(false)
      throw error
    }
  }, [onLastScanHint, serviceId, settleRollbackTargetSnapshot, stackId])

  const refreshStackOnly = useCallback(async (source = 'stack-refresh') => {
    const requestId = ++stackRefreshRequestIdRef.current; let rollbackSnapshotMayBeStale = false
    try {
      const st = await getStack(stackId)
      if (requestId < latestAppliedStackRefreshRequestIdRef.current) return
      latestAppliedStackRefreshRequestIdRef.current = requestId
      const svc = st.services.find((s) => s.id === serviceId) ?? null
      setStack(st)
      setService(svc)
      setRollbackTargetRefreshing(Boolean(svc && !isDockrevService(svc)))
      if (!svc || isDockrevService(svc)) {
        setRollbackTarget(null); setRollbackActiveTarget(null)
        setRollbackTargetRefreshing(false)
        return
      }
      rollbackSnapshotMayBeStale = true
      const target = await getServiceRollbackTarget(serviceId)
      await settleRollbackTargetSnapshot(requestId, svc, target, source)
    } catch (error: unknown) {
      if (requestId < latestAppliedStackRefreshRequestIdRef.current) return
      if (rollbackSnapshotMayBeStale) { setRollbackTarget(null); if (source !== 'rollback-active-poll') setRollbackActiveTarget(null) }
      setRollbackTargetRefreshing(false)
      throw error
    }
  }, [serviceId, settleRollbackTargetSnapshot, stackId])

  const patchServiceInStack = useCallback(
    (patch: (svc: Service) => Service) => {
      setStack((prev) => {
        if (!prev) return prev
        let changed = false
        const nextServices = prev.services.map((svc) => {
          if (svc.id !== serviceId) return svc
          changed = true
          return patch(svc)
        })
        if (!changed) return prev
        return { ...prev, services: nextServices }
      })

      setService((prev) => {
        if (!prev || prev.id !== serviceId) return prev
        return patch(prev)
      })
    },
    [serviceId],
  )

  const requestRefresh = usePageResumeRefresh(refresh, {
    onError: (e: unknown) => setError(errorMessage(e)),
  })

  useEffect(() => {
    void requestRefresh().catch((e: unknown) => setError(errorMessage(e)))
  }, [requestRefresh, serviceId, stackId])

  useEffect(() => {
    let closed = false
    const timers = new Set<number>()

    const handleRefreshError = (error: unknown) => {
      if (closed) return
      setError(errorMessage(error))
    }

    const schedule = (task: () => Promise<void>) => {
      const timer = window.setTimeout(() => {
        timers.delete(timer)
        void task().catch(handleRefreshError)
      }, UPDATE_JOB_SETTLE_RETRY_MS)
      timers.add(timer)
    }

    const onUpdateJobSettled = (evt: Event) => {
      const detail = evt instanceof CustomEvent ? (evt.detail as UpdateJobSettledDetail | null) : null
      if (!detail) return

      const matchesCurrent =
        detail.scope === 'all' ||
        detail.target === 'all' ||
        detail.stackId === stackId ||
        detail.serviceId === serviceId ||
        detail.target === `stack:${stackId}` ||
        detail.target === `service:${serviceId}`
      if (!matchesCurrent) return

      void refreshStackOnly('update-job-settled').catch(handleRefreshError)
      schedule(async () => {
        await refreshStackOnly('update-job-settled-retry')
      })
    }

    window.addEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateJobSettled)
    return () => {
      closed = true
      for (const timer of timers) window.clearTimeout(timer)
      window.removeEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateJobSettled)
    }
  }, [refreshStackOnly, serviceId, stackId])

  useEffect(() => {
    const activeJobId = rollbackActiveJobId
    if (!activeJobId) return

    let cancelled = false
    let timer: number | null = null

    const tick = async () => {
      try {
        await refreshStackOnly('rollback-active-poll')
      } catch {
        // best-effort polling while rollback-related job is active
      } finally {
        if (!cancelled) {
          timer = window.setTimeout(() => {
            void tick()
          }, 1200)
        }
      }
    }

    timer = window.setTimeout(() => {
      void tick()
    }, 1200)

    return () => {
      cancelled = true
      if (timer != null) window.clearTimeout(timer)
    }
  }, [refreshStackOnly, rollbackActiveJobId])

  useRollbackTargetInvariantWarning(service, rollbackTarget)

  const applyDigestSnapshotUpdate = useCallback(
    (detail: DigestSnapshotUpdatedDetail) => {
      const imageRepo = (detail.imageRepo ?? '').trim().toLowerCase()
      const digestNorm = normalizeDigest(detail.digest)?.toLowerCase() ?? null
      const triggerServiceId = (detail.triggerServiceId ?? '').trim()
      if (!imageRepo || !digestNorm) return
      if (!triggerServiceId || triggerServiceId !== serviceId) return

      const failures = scanHasFailures(detail.scan)
      const complete = scanIsComplete(detail.scan)

      patchServiceInStack((prev) => {
        const svcRepo = imageRepoFromImageRef(prev.image.ref)
        if (!svcRepo || svcRepo !== imageRepo) return prev

        let changed = false
        let next: Service = prev

        const currentDigest = normalizeDigest(prev.image.digest)?.toLowerCase() ?? null
        if (currentDigest && currentDigest === digestNorm && !isStrictSemverTag(prev.image.tag)) {
          const inferred = inferResolvedTagsFromSnapshot(detail.tags, prev.image.tag)
          const inferredFirst = inferred[0] ?? null
          if (inferredFirst || (!failures && complete)) {
            changed = true
            next = {
              ...next,
              image: {
                ...next.image,
                resolvedTag: inferredFirst,
                resolvedTags: inferred.length > 1 ? inferred : null,
              },
            }
          }
        }

        const candidate = next.candidate
        const candidateDigest = candidate ? normalizeDigest(candidate.digest)?.toLowerCase() ?? null : null
        if (candidate && candidateDigest && candidateDigest === digestNorm && !isStrictSemverTag(candidate.tag)) {
          const inferred = inferResolvedTagsFromSnapshot(detail.tags, candidate.tag)
          const inferredFirst = inferred[0] ?? null
          if (inferredFirst || (!failures && complete)) {
            changed = true
            next = {
              ...next,
              candidate: { ...candidate, resolvedTag: inferredFirst },
            }
          }
        }

        return changed ? next : prev
      })
    },
    [patchServiceInStack, serviceId],
  )

  useEffect(() => {
    if (typeof window === 'undefined') return
    const onDigestSnapshotUpdated = (evt: Event) => {
      const detail =
        evt instanceof CustomEvent
          ? (evt.detail as DigestSnapshotUpdatedDetail | null)
          : null
      if (!detail) return
      applyDigestSnapshotUpdate(detail)
    }
    window.addEventListener(DIGEST_SNAPSHOT_UPDATED_EVENT, onDigestSnapshotUpdated)
    return () => {
      window.removeEventListener(DIGEST_SNAPSHOT_UPDATED_EVENT, onDigestSnapshotUpdated)
    }
  }, [applyDigestSnapshotUpdate])

  useEffect(() => {
    if (service?.versionInference?.status !== 'pending') return
    let closed = false
    let timer: number | null = null

    const poll = async () => {
      await refreshStackOnly().catch(() => {})
      if (closed) return
      timer = window.setTimeout(() => {
        void poll()
      }, 1200)
    }

    timer = window.setTimeout(() => {
      void poll()
    }, 1200)

    return () => {
      closed = true
      if (timer != null) window.clearTimeout(timer)
    }
  }, [refreshStackOnly, service?.versionInference?.status])

  useEffect(() => {
    let closed = false
    let es: EventSource | null = null
    let timer: number | null = null

    const scheduleRefresh = () => {
      if (timer != null) return
      timer = window.setTimeout(() => {
        timer = null
        void refreshStackOnly().catch(() => {})
      }, 200)
    }

    const start = async () => {
      let jobId: string | null = null
      try {
        const resp = await triggerRuntimeScan('all')
        jobId = resp.jobId
      } catch (e: unknown) {
        if (e instanceof ApiError && e.status === 409) {
          const d = e.details
          const existingJobId =
            d && typeof d === 'object' && d !== null && 'existingJobId' in d && typeof (d as Record<string, unknown>).existingJobId === 'string'
              ? ((d as Record<string, unknown>).existingJobId as string)
              : null
          jobId = existingJobId
        }
      }

      if (closed || !jobId) return
      es = newJobEventsSource(jobId)

      es.addEventListener('runtime_scan_service', (evt: Event) => {
        const data = (evt as MessageEvent).data
        if (typeof data !== 'string' || !data) return
        try {
          const parsed = JSON.parse(data) as unknown
          if (!parsed || typeof parsed !== 'object') return
          const p = parsed as Record<string, unknown>
          if (p.type !== 'runtime_scan_service') return
          if (p.changed !== true) return
          const eventStackId = typeof p.stackId === 'string' ? p.stackId : ''
          if (eventStackId && eventStackId === stackId) scheduleRefresh()
        } catch {
          // ignore invalid events
        }
      })

      es.addEventListener('runtime_scan_finished', () => {
        es?.close()
        void refreshStackOnly().catch(() => {})
      })
    }

    void start()

    return () => {
      closed = true
      if (timer != null) window.clearTimeout(timer)
      es?.close()
    }
  }, [refreshStackOnly, stackId])

  const archiveOrRestoreService = useCallback(async () => {
    if (!service) return
    setBusy(true)
    setError(null)
    try {
      if (service.archived) {
        await restoreService(service.id)
      } else {
        await archiveService(service.id)
      }
      await requestRefresh()
    } catch (e: unknown) {
      setError(errorMessage(e))
    } finally {
      setBusy(false)
    }
  }, [requestRefresh, service])

  const blockServiceUpdates = useCallback(async () => {
    setBusy(true)
    setError(null)
    try {
      await createIgnore({
        enabled: true,
        serviceId,
        kind: 'regex',
        value: '.*',
        note: 'blocked via UI',
      })
      await requestRefresh()
    } catch (e: unknown) {
      setError(errorMessage(e))
    } finally {
      setBusy(false)
    }
  }, [requestRefresh, serviceId])

  const topActions = useMemo(
    () => (
      <>
        {service && isDockrevService(service) ? (
          <>
            <Button
              variant="primary"
              disabled={busy || supervisorState.status !== 'ok'}
              title={
                supervisorState.status === 'offline'
                  ? `自我升级不可用（supervisor offline） · ${supervisorErrorAt ?? '-'} · ${supervisorError ?? '-'}`
                  : supervisorState.status === 'checking'
                    ? '检查 supervisor 中…'
                    : undefined
              }
              onClick={() => {
                window.location.href = selfUpgradeUrl
              }}
            >
              升级 Dockrev
            </Button>
            {supervisorState.status !== 'ok' ? (
              <Button
                variant="ghost"
                disabled={busy || supervisorState.status === 'checking'}
                onClick={() => {
                  void checkSupervisor()
                }}
              >
                重试
              </Button>
            ) : null}
          </>
        ) : (
          <>
            <Button
              variant="primary"
              disabled={
                busy ||
                !service ||
                service.ignore?.matched ||
                !service.candidate ||
                service.candidate.archMatch === 'mismatch'
              }
              title={
                !service
                  ? undefined
                  : service.ignore?.matched
                    ? service.ignore.reason ?? '被阻止'
                    : !service.candidate
                      ? '无候选版本'
                      : service.candidate.archMatch === 'mismatch'
                        ? '架构不匹配（仅提示，不允许更新）'
                        : undefined
              }
              onClick={() => {
                void (async () => {
                  if (!service || !service.candidate) return
                  setBusy(true)
                  setError(null)
                  setNotice(null)
                  try {
                    const resp = await triggerUpdate({
                      scope: 'service',
                      stackId,
                      ...(await buildUpdateServiceTarget(service)),
                      mode: 'dry-run',
                      allowArchMismatch: false,
                      backupMode: 'inherit',
                    })
                    setNotice({ jobId: resp.jobId, kind: 'update' })
                  } catch (e: unknown) {
                    if (e instanceof ApiError) {
                      if (e.status === 401) setError('需要登录/鉴权（Forward Auth）')
                      else if (e.status === 409) {
                        setError('扫描结果已变化，请刷新并重新扫描后再更新')
                        await requestRefresh()
                      } else setError(e.message)
                    } else {
                      setError(errorMessage(e))
                    }
                  } finally {
                    setBusy(false)
                  }
                })()
              }}
            >
              预览更新
            </Button>
            <Button
              variant="primary"
              disabled={
                applySubmitting && !applyActiveJob
                  ? true
                  : applyActiveJob
                    ? false
                    : busy ||
                      !service ||
                      serviceRowStatus(service) === 'blocked' ||
                      !service.candidate ||
                      service.candidate.archMatch === 'mismatch'
              }
              loading={applyActionBusy}
              loadingClickable={Boolean(applyActiveJob)}
              hint={applyActiveJob ? '任务进行中，点击查看任务详情' : undefined}
              title={
                applyActiveJob
                  ? '任务进行中，点击查看任务详情'
                  : !service
                    ? undefined
                    : serviceRowStatus(service) === 'blocked'
                      ? blockedReasonFor(service) ?? '被阻止'
                      : !service.candidate
                        ? '无候选版本'
                        : service.candidate.archMatch === 'mismatch'
                          ? '架构不匹配（仅提示，不允许更新）'
                          : undefined
              }
              onClick={() => {
                void (async () => {
                  if (applyActiveJob) {
                    navigate({ name: 'job', jobId: applyActiveJob.jobId })
                    return
                  }
                  if (!service || !service.candidate) return
                  const ok = await confirm({
                    title: `确认更新服务 ${service.name}？`,
                    body: (
                      <ServiceUpdateConfirmDetails
                        service={service}
                        status={serviceRowStatus(service)}
                        onHostResolvedTags={(update) => {
                          patchServiceInStack((prev) => ({
                            ...prev,
                            image: {
                              ...prev.image,
                              resolvedTag: update.resolvedTag,
                              resolvedTags: update.resolvedTags,
                            },
                          }))
                        }}
                        onHostCandidateResolvedTag={(resolvedTag) => {
                          patchServiceInStack((prev) => ({
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
                    ),
                    confirmText: '执行更新',
                    cancelText: '取消',
                    confirmVariant: 'primary',
                    badgeText: null,
                  })
                  if (!ok) return
                  setError(null)
                  setNotice(null)
                  if (applyActionKey) beginSubmitting(applyActionKey)
                  try {
                    const resp = await triggerUpdate({
                      scope: 'service',
                      stackId,
                      ...(await buildUpdateServiceTarget(service)),
                      mode: 'apply',
                      allowArchMismatch: false,
                      backupMode: 'inherit',
                    })
                    setNotice({ jobId: resp.jobId, kind: 'update' })
                    if (applyActionKey) trackJob(applyActionKey, resp.jobId, 'queued')
                  } catch (e: unknown) {
                    if (e instanceof ApiError) {
                      if (e.status === 401) setError('需要登录/鉴权（Forward Auth）')
                      else if (e.status === 409) {
                        const guardReason = readUpdateGuardBlockedReason(e)
                        if (guardReason) setError(guardReason)
                        else {
                          setError('扫描结果已变化，请刷新并重新扫描后再更新')
                          await requestRefresh()
                        }
                      } else setError(e.message)
                    } else {
                      setError(errorMessage(e))
                    }
                  } finally {
                    if (applyActionKey) endSubmitting(applyActionKey)
                  }
                })()
              }}
            >
              {applyActiveJob?.status === 'queued'
                ? '排队中…'
                : applyActiveJob
                  ? '更新中…'
                  : applySubmitting
                    ? '提交中…'
                    : '执行更新'}
            </Button>
            <Button
              variant="ghost"
              disabled={rollbackActiveJobId ? false : busy || rollbackTargetRefreshing || !rollbackTarget?.available}
              loading={rollbackActionBusy || rollbackTargetRefreshing}
              loadingClickable={Boolean(rollbackActiveJobId)}
              hint={rollbackHint}
              title={rollbackActiveJobId ? '任务进行中，点击查看任务详情' : undefined}
              onClick={() => {
                void (async () => {
                  if (rollbackActiveJobId) {
                    navigate({ name: 'job', jobId: rollbackActiveJobId })
                    return
                  }
                  if (!service || !rollbackTarget?.available || !rollbackTarget.targetDigest) return
                  const ok = await confirm({
                    title: `确认回滚服务 ${service.name}？`,
                    body: (
                      <>
                        <div className="modalLead">将把该服务回滚到上次升级前的版本。</div>
                        <div className="modalKvGrid">
                          <div className="modalKvLabel">范围</div>
                          <div className="modalKvValue">
                            <Mono>service</Mono>
                          </div>
                          <div className="modalKvLabel">目标</div>
                          <div className="modalKvValue">
                            <Mono>{`${stack?.name ?? stackId}/${service.name}`}</Mono>
                          </div>
                          <div className="modalKvLabel">当前版本</div>
                          <div className="modalKvValue">
                            <span>{rollbackVersionLabel(rollbackTarget.currentDisplayTag, rollbackTarget.currentDigest)}</span>
                            <span className="muted">{` · ${shortDigest(rollbackTarget.currentDigest)}`}</span>
                          </div>
                          <div className="modalKvLabel">回滚目标</div>
                          <div className="modalKvValue">
                            <span>{rollbackVersionLabel(rollbackTarget.targetDisplayTag, rollbackTarget.targetDigest)}</span>
                            <span className="muted">{` · ${shortDigest(rollbackTarget.targetDigest)}`}</span>
                          </div>
                          <div className="modalKvLabel">来源任务</div>
                          <div className="modalKvValue">
                            <Mono>{rollbackTarget.sourceUpdateJobId ?? '-'}</Mono>
                          </div>
                          <div className="modalKvLabel">完成时间</div>
                          <div className="modalKvValue">
                            <Mono>{rollbackTarget.sourceFinishedAt ?? '-'}</Mono>
                          </div>
                        </div>
                        <div className="modalDivider" />
                      </>
                    ),
                    confirmText: '执行回滚',
                    cancelText: '取消',
                    confirmVariant: 'danger',
                    badgeText: null,
                  })
                  if (!ok) return
                  setBusy(true)
                  setError(null)
                  setNotice(null)
                  try {
                    const resp = await triggerServiceRollback(service.id)
                    setNotice({ jobId: resp.jobId, kind: 'rollback' })
                    await refreshStackOnly()
                  } catch (e: unknown) {
                    if (e instanceof ApiError) {
                      if (e.status === 401) setError('需要登录/鉴权（Forward Auth）')
                      else if (e.status === 409) {
                        const details = e.details
                        const existingJobId =
                          details && typeof details === 'object' && details !== null && 'existingJobId' in details
                            ? (details as Record<string, unknown>).existingJobId
                            : null
                        const reason =
                          details && typeof details === 'object' && details !== null && 'reason' in details
                            ? (details as Record<string, unknown>).reason
                            : null
                        if (typeof existingJobId === 'string' && existingJobId.trim()) {
                          navigate({ name: 'job', jobId: existingJobId })
                        } else if (typeof reason === 'string' && reason.trim()) {
                          setError(rollbackUnavailableReasonLabel(reason) ?? e.message)
                        } else {
                          setError(e.message)
                        }
                        await refreshStackOnly()
                      } else setError(e.message)
                    } else {
                      setError(errorMessage(e))
                    }
                  } finally {
                    setBusy(false)
                  }
                })()
              }}
            >
              {rollbackActiveJobId
                ? rollbackReason === 'rollback_in_progress'
                  ? rollbackActiveJobStatus === 'queued'
                    ? '排队中…'
                    : '回滚中…'
                  : rollbackActiveJobStatus === 'queued'
                    ? '排队中…'
                    : '任务进行中…'
                : rollbackTargetRefreshing
                  ? '刷新中…'
                  : '回滚'}
            </Button>
          </>
        )}
        <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'stack', stackId })}>
          Stack 详情
        </Button>
      </>
    ),
    [
      applyActiveJob,
      applyActionBusy,
      applyActionKey,
      applySubmitting,
      beginSubmitting,
      busy,
      checkSupervisor,
      confirm,
      endSubmitting,
      patchServiceInStack,
      requestRefresh,
      refreshStackOnly,
      rollbackActionBusy,
      rollbackActiveJobId,
      rollbackActiveJobStatus,
      rollbackHint,
      rollbackReason,
      rollbackTarget,
      rollbackTargetRefreshing,
      selfUpgradeUrl,
      service,
      stack?.name,
      stackId,
      supervisorError,
      supervisorErrorAt,
      supervisorState.status,
      trackJob,
    ],
  )

  const dangerousActions = useMemo(
    () => (
      <>
        <Button
          variant={service?.archived ? 'primary' : 'ghost'}
          disabled={busy || !service}
          onClick={() => {
            void archiveOrRestoreService()
          }}
        >
          {service?.archived ? '恢复' : '归档'}
        </Button>
        <Button
          variant="danger"
          disabled={busy}
          onClick={() => {
            void blockServiceUpdates()
          }}
        >
          阻止此服务更新
        </Button>
      </>
    ),
    [archiveOrRestoreService, blockServiceUpdates, busy, service],
  )

  const bindTargets = useMemo(() => (settings ? formatMap(settings.backupTargets.bindPaths) : []), [settings])
  const volTargets = useMemo(() => (settings ? formatMap(settings.backupTargets.volumeNames) : []), [settings])
  const draftRepoUrl = useMemo(() => normalizeExternalHttpUrl(settings?.repoUrl), [settings?.repoUrl])
  const settingsBusy = busy || repoInferBusy

  const tone = useMemo(() => (service ? svcTone(service) : 'muted'), [service])
  const bannerClass =
    tone === 'ok' ? 'svcBanner svcBannerOk' : tone === 'warn' ? 'svcBanner svcBannerWarn' : tone === 'bad' ? 'svcBanner svcBannerBad' : 'svcBanner svcBannerMuted'
  const dotClass =
    tone === 'ok'
      ? 'svcBannerDot svcBannerDotOk'
      : tone === 'warn'
        ? 'svcBannerDot svcBannerDotWarn'
        : tone === 'bad'
          ? 'svcBannerDot svcBannerDotBad'
          : 'svcBannerDot'

  const bannerTitle = useMemo(() => {
    if (!service) return '加载中…'
    const st = serviceRowStatus(service)
    if (st === 'blocked') {
      return '已阻止（忽略规则命中）'
    }
    if (st === 'ok') return '暂无候选版本'
    if (st === 'archMismatch') return '架构不匹配（仅提示，不允许更新）'
    if (st === 'hint') return '需确认（arch 未知）'
    return '可更新'
  }, [service])

  const bannerDetail = useMemo<ReactNode>(() => {
    if (!service) return null

    const currentTag = formatTagDisplay(
      service.image.tag,
      service.image.resolvedTag,
      service.versionInference?.status,
    )
    const inferencePending = service.versionInference?.status === 'pending'
    const currentDigestNode = service.image.digest ? (
      <span className="mono">{`@${shortDigest(service.image.digest)}`}</span>
    ) : null

    const currentNode = (
      <CurrentVersionPopover
        serviceId={service.id}
        displayTag={currentTag}
        imageTag={service.image.tag}
        imageDigest={service.image.digest ?? null}
        resolvedTag={service.image.resolvedTag}
        resolvedTags={service.image.resolvedTags}
        onLocalResolvedTags={(update) => {
          patchServiceInStack((prev) => ({
            ...prev,
            image: {
              ...prev.image,
              resolvedTag: update.resolvedTag,
              resolvedTags: update.resolvedTags,
            },
          }))
        }}
        inferenceLoading={inferencePending}
      />
    )

    const rawTagTrim = (service.image.tag ?? '').trim()
    const showRawTag = Boolean(rawTagTrim && rawTagTrim !== currentTag)
    const rawTagNode = showRawTag ? (
      <>
        {' · '}raw:{' '}
        <CurrentVersionPopover
          serviceId={service.id}
          displayTag={service.image.tag}
          imageTag={service.image.tag}
          imageDigest={service.image.digest ?? null}
          resolvedTag={service.image.resolvedTag}
          resolvedTags={service.image.resolvedTags}
          onLocalResolvedTags={(update) => {
            patchServiceInStack((prev) => ({
              ...prev,
              image: {
                ...prev.image,
                resolvedTag: update.resolvedTag,
                resolvedTags: update.resolvedTags,
              },
            }))
          }}
          preferSource="rawTag"
          triggerClassName="versionTagsTrigger mono monoSecondary"
        >
          {service.image.tag}
        </CurrentVersionPopover>
      </>
    ) : null

    if (service.ignore?.matched) {
      return (
        <>
          当前: {currentNode}
          {currentDigestNode}
          {rawTagNode}
          {' · '}rule: <Mono>{service.ignore.ruleId}</Mono>
          {service.ignore.reason ? (
            <>
              {' · '}reason: <Mono>{service.ignore.reason}</Mono>
            </>
          ) : null}
        </>
      )
    }

    if (!service.candidate) {
      return (
        <>
          当前: {currentNode}
          {currentDigestNode}
          {rawTagNode}
        </>
      )
    }

    const candidateDisplayTag = formatCandidateTagDisplay(
      service.candidate.tag,
      service.candidate.resolvedTag ?? null,
      service.versionInference?.status,
    )
    const candidatePrefetchOnMount = shouldPrefetchFloatingCandidate(
      service.candidate.tag,
      service.candidate.resolvedTag ?? null,
      service.candidate.digest ?? null,
    )
    const archNode = service.candidate.arch.length ? (
      <>
        {' · '}arch=<Mono>{service.candidate.arch.join(',')}</Mono>
      </>
    ) : null
    return (
      <>
        当前: {currentNode}
        {currentDigestNode}
        {rawTagNode}
	        {' \u2192 '}候选:{' '}
		        <VersionTagsPopover
		          serviceId={service.id}
		          candidateTag={service.candidate.tag}
		          candidateDigest={service.candidate.digest ?? null}
	            prefetchOnMount={candidatePrefetchOnMount}
		          onLocalResolvedTag={(resolvedTag) => {
		            patchServiceInStack((prev) => ({
		              ...prev,
		              candidate: prev.candidate
		                ? {
		                    ...prev.candidate,
		                    resolvedTag,
		                  }
		                : prev.candidate,
		            }))
		          }}
		        >
		          {candidateDisplayTag}
		        </VersionTagsPopover>
	        <span className="mono">{`@${shortDigest(service.candidate.digest)}`}</span>
	        {archNode}
	      </>
    )
  }, [patchServiceInStack, service])

  const rawComposeType = typeof stack?.compose?.type === 'string' ? stack.compose.type.trim() : ''
  const composeType = rawComposeType || '-'
  const composeFilesRaw = Array.isArray(stack?.compose?.composeFiles) ? stack.compose.composeFiles : []
  const composeFiles = composeFilesRaw
    .map((item) => (typeof item === 'string' ? item.trim() : ''))
    .filter((item) => item.length > 0)
  const composeEnvFileRaw = typeof stack?.compose?.envFile === 'string' ? stack.compose.envFile.trim() : ''
  const composeEnvFile = composeEnvFileRaw || '-'
  const semverDowngradeAnomaly = service ? isSemverDowngradeAnomaly(service) : false
  const anomalyCurrentTag = formatTagDisplay(
    service?.image.tag ?? '',
    service?.image.resolvedTag,
    service?.versionInference?.status,
  )
  const anomalyCandidateTag = service?.candidate
    ? formatCandidateTagDisplay(
        service.candidate.tag,
        service.candidate.resolvedTag ?? null,
        service.versionInference?.status,
      )
    : '-'
  return {
    anomalyCandidateTag,
    anomalyCurrentTag,
    bannerClass,
    bannerDetail,
    bannerTitle,
    bindTargets,
    busy,
    composeEnvFile,
    composeFiles,
    composeType,
    dotClass,
    draftRepoUrl,
    error,
    newRuleKind,
    newRuleNote,
    newRuleValue,
    notice,
    repoInferBusy,
    requestRefresh,
    rules,
    semverDowngradeAnomaly,
    service,
    serviceId,
    setBusy,
    setError,
    setNewRuleKind,
    setNewRuleNote,
    setNewRuleValue,
    setRepoInferBusy,
    setSettings,
    settings,
    stackSettings,
    settingsBusy,
    stack,
    supervisorErrorAt,
    supervisorState,
    topActions,
    tone,
    dangerousActions,
    volTargets,
  }
}
