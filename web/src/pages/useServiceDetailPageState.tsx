import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ArrowUpCircle, Download, Eye, Layers3, Play, RotateCcw, RotateCw, Square } from 'lucide-react'
import { ApiError, archiveService, createIgnore, getServiceBackupRecords, getServiceBackupTargets, getServiceLifecycleStatus, getServiceRollbackTarget, getServiceSettings, getStack, getStackSettings, listIgnores, restoreService, triggerServiceLifecycle, triggerServiceRollback, triggerUpdate, type IgnoreRule, type Service, type ServiceBackupRecordItem, type ServiceBackupTargetsResponse, type ServiceLifecycleAction, type ServiceLifecycleStatusResponse, type ServiceRollbackTargetResponse, type ServiceSettings, type StackDetail, type StackSettings } from '../api'
import { readUpdateGuardBlockedReason } from '../aggregateUpdateGuard'
import { normalizeDigest } from '../components/digest'
import { backupSummaryValue, summarizeServiceOperationBackups } from '../components/serviceOperationBackupSummary'
import { ServiceUpdateConfirmDetails } from '../components/ServiceUpdateConfirmDetails'
import { ServiceMobileActionMenu, ServiceSplitActionButton, ServiceStackDetailAction } from '../components/ServiceSplitActionButton'
import { useConfirm } from '../confirm'
import { DIGEST_SNAPSHOT_UPDATED_EVENT, type DigestSnapshotUpdatedDetail } from '../digestInferenceTracker'
import { normalizeExternalHttpUrl } from '../imageLinks'
import { imageRepoFromImageRef } from '../imageRepo'
import { publishServiceTreeRefresh } from '../serviceTreeRefresh'
import { describeServiceOperationProgress } from '../serviceOperationProgress'
import { activeServiceOperation, conflictingJobId, dockrevSelfUpgradeBusyReason, errorMessage, isDockrevService, normalizeMaybeDigest, openSelfUpgradeUrl, rollbackTargetMatchesServiceDigest, rollbackUnavailableReasonLabel, rollbackVersionLabel, ROLLBACK_TARGET_REFRESH_HINT, scanHasFailures, scanIsComplete, serviceOperationOwner, shortDigest, svcTone, useRollbackTargetInvariantWarning } from './serviceDetailUtils'
import { navigate } from '../routes'
import { selfUpgradeBaseUrl } from '../runtimeConfig'
import { Button, Mono } from '../ui'
import { UPDATE_JOB_SETTLED_EVENT, resolveUpdateActionTargetKey, useUpdateActionTracker, type UpdateActionTargetKey, type UpdateJobSettledDetail } from '../updateActionTracking'
import { blockedReasonFor, isSemverDowngradeAnomaly, serviceRowStatus } from '../updateStatus'
import { buildUpdateServiceTarget } from '../updateTargets'
import { useManagementEventBatch } from '../managementEvents'
import { usePageResumeRefresh } from '../usePageResumeRefresh'
import { useSupervisorHealth } from '../useSupervisorHealth'
import { formatCandidateTagDisplay, formatCurrentTagDisplay as formatTagDisplay, inferResolvedTagsFromSnapshot, isStrictSemverTag } from '../versionDisplay'
import { isRollbackTargetRefreshCurrent, retryRollbackTargetDigestMismatch } from './rollbackTargetRefresh'
import { managementEventAffectsServiceDetail } from './serviceDetailManagement'
import type { AsyncDataPhase } from '../asyncData'
export { managementEventAffectsServiceDetail } from './serviceDetailManagement'

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
  const [settingsPhase, setSettingsPhase] = useState<AsyncDataPhase>('initial-loading')
  const [backupTargets, setBackupTargets] = useState<ServiceBackupTargetsResponse | null>(null)
  const [backupRecords, setBackupRecords] = useState<ServiceBackupRecordItem[]>([])
  const [backupPhase, setBackupPhase] = useState<AsyncDataPhase>('initial-loading')
  const [backupLoaded, setBackupLoaded] = useState(false)
  const [backupLoadError, setBackupLoadError] = useState<string | null>(null)
  const [stackSettings, setStackSettings] = useState<StackSettings | null>(null)
  const [rules, setRules] = useState<IgnoreRule[]>([])
  const [busy, setBusy] = useState(false)
  const [repoInferBusy, setRepoInferBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<{ jobId: string; kind: 'update' | 'rollback' | 'lifecycle' } | null>(null)
  const [rollbackTarget, setRollbackTarget] = useState<ServiceRollbackTargetResponse | null>(null)
  const [rollbackActiveTarget, setRollbackActiveTarget] = useState<ServiceRollbackTargetResponse | null>(null)
  const { beginSubmitting, endSubmitting, trackJob, getActiveJobByTarget, isTargetSubmitting } =
    useUpdateActionTracker()
  const { state: supervisorState, check: checkSupervisor } = useSupervisorHealth()
  const supervisorErrorAt = supervisorState.status === 'offline' ? supervisorState.errorAt : undefined
  const supervisorError = supervisorState.status === 'offline' ? supervisorState.error : undefined
  const selfUpgradeUrl = useMemo(() => selfUpgradeBaseUrl(), [])
  const applyActionKey = useMemo(
    () => resolveUpdateActionTargetKey('service', stackId, serviceId),
    [serviceId, stackId],
  )
  const applyActiveJob = applyActionKey ? getActiveJobByTarget(applyActionKey) : null
  const applySubmitting = applyActionKey ? isTargetSubmitting(applyActionKey) : false
  const [rollbackTargetRefreshing, setRollbackTargetRefreshing] = useState(false)
  const [lifecycleStatus, setLifecycleStatus] = useState<ServiceLifecycleStatusResponse | null>(null)
  const [lifecycleSettledJobId, setLifecycleSettledJobId] = useState<string | null>(null)
  const lifecycleActiveJobIdRef = useRef<string | null>(null)
  const lifecycleStatusRequestIdRef = useRef(0)
  const rollbackActiveJobIdRef = useRef<string | null>(null)
  const submittingTokensRef = useRef(new Map<symbol, UpdateActionTargetKey>())
  const [lastSuccessfulRefreshAt, setLastSuccessfulRefreshAt] = useState<string | null>(null)
  const lifecycleJob = lifecycleStatus?.activeJob ?? null
  const lifecycleOwner = lifecycleJob && activeServiceOperation(lifecycleJob.status)
    ? serviceOperationOwner(lifecycleJob.type)
    : null
  const activeOperation = useMemo(
    () => lifecycleOwner === 'update' && applyActiveJob
      ? { owner: 'update' as const, id: applyActiveJob.jobId, status: applyActiveJob.status, action: null, targetVersion: applyActiveJob.targetVersion }
      : lifecycleJob && lifecycleOwner
      ? { owner: lifecycleOwner, id: lifecycleJob.id, status: lifecycleJob.status, action: lifecycleJob.action ?? null, ...(lifecycleOwner === 'update' ? { targetVersion: applyActiveJob?.targetVersion } : {}) }
      : applyActiveJob
        ? { owner: 'update' as const, id: applyActiveJob.jobId, status: applyActiveJob.status, action: null, targetVersion: applyActiveJob.targetVersion }
        : null,
    [applyActiveJob, lifecycleJob, lifecycleOwner],
  )
  const activeOperationOwner = activeOperation?.owner ?? null
  const activeUpdateJob = activeOperationOwner === 'update' ? activeOperation : null
  const activeRollbackJob = activeOperationOwner === 'rollback' ? activeOperation : null
  const activeLifecycleJob = activeOperationOwner === 'lifecycle' ? activeOperation : null
  const rollbackActiveJobId = activeRollbackJob?.id ?? null
  const rollbackActiveJobStatus = activeRollbackJob?.status ?? null
  const operationProgress = useMemo(
    () => describeServiceOperationProgress({
      updateSubmitting: applySubmitting,
      updateStatus: activeUpdateJob?.status,
      rollbackStatus: rollbackActiveJobStatus,
    }),
    [activeUpdateJob?.status, applySubmitting, rollbackActiveJobStatus],
  )
  const rollbackReason = rollbackTarget?.unavailableReason ?? rollbackActiveTarget?.unavailableReason ?? null
  const rollbackReasonLabel = rollbackUnavailableReasonLabel(rollbackReason)
  const rollbackHint = rollbackActiveJobId ? '任务进行中，点击查看任务详情' : rollbackTargetRefreshing ? ROLLBACK_TARGET_REFRESH_HINT : !rollbackTarget?.available ? rollbackReasonLabel : undefined
  const backupSummaryByJobId = useMemo(() => summarizeServiceOperationBackups(backupRecords), [backupRecords])
  const rollbackBackupSummary = useMemo(() => {
    const sourceJobId = (rollbackTarget?.sourceUpdateJobId ?? '').trim()
    if (!sourceJobId) return { state: 'empty' as const }
    return backupSummaryByJobId.get(sourceJobId) ?? { state: 'empty' as const }
  }, [backupSummaryByJobId, rollbackTarget?.sourceUpdateJobId])
  const rollbackBackupValue = rollbackBackupSummary.state === 'empty' ? null : backupSummaryValue(rollbackBackupSummary)
  const fullRefreshRequestIdRef = useRef(0)
  const latestAppliedFullRefreshRequestIdRef = useRef(0)
  const stackRefreshRequestIdRef = useRef(0)
  const latestAppliedStackRefreshRequestIdRef = useRef(0)
  const pageGenerationRef = useRef(0); const pageGeneration = pageGenerationRef.current
  const rollbackTargetRef = useRef(rollbackTarget)
  const rollbackTargetRefreshingRef = useRef(rollbackTargetRefreshing)
  const serviceRef = useRef(service)
  const settingsRef = useRef(settings)
  const backupRecordsRef = useRef(backupRecords)
  const backupHasCommittedDataRef = useRef(false)
  rollbackTargetRef.current = rollbackTarget
  rollbackTargetRefreshingRef.current = rollbackTargetRefreshing
  serviceRef.current = service
  settingsRef.current = settings
  backupRecordsRef.current = backupRecords

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
    if (!isRollbackTargetRefreshCurrent(requestId, stackRefreshRequestIdRef.current, latestAppliedStackRefreshRequestIdRef.current)) {
      warnRollbackTargetDiscard('outdated_request', requestId, svc, target, source)
      return 'outdated'
    }
    if (svc && !isDockrevService(svc) && target && !rollbackTargetMatchesServiceDigest(svc, target)) {
      warnRollbackTargetDiscard('current_digest_mismatch', requestId, svc, target, source)
      if (target?.activeJobId) setRollbackActiveTarget(target)
      setRollbackTargetRefreshing(true)
      return 'digest_mismatch'
    }
    setRollbackTarget(target); setRollbackActiveTarget(target?.activeJobId ? target : null); setError((prev) => prev === '回滚信息刷新失败，请稍后重试' ? null : prev)
    setRollbackTargetRefreshing(false)
    return 'applied'
  }, [warnRollbackTargetDiscard])

  const settleRollbackTargetSnapshot = useCallback(async (requestId: number, svc: Service, target: ServiceRollbackTargetResponse | null) => {
    const result = await retryRollbackTargetDigestMismatch({
      initialTarget: target,
      requestId,
      currentRequestId: () => stackRefreshRequestIdRef.current,
      validate: (nextTarget) => {
        const snapshotResult = applyRollbackTargetSnapshot(requestId, svc, nextTarget, 'snapshot')
        return snapshotResult === 'digest_mismatch' ? 'digest_mismatch' : snapshotResult === 'outdated' ? 'outdated' : 'matched'
      },
      fetchTarget: () => getServiceRollbackTarget(serviceId),
      sleep: (delayMs) => new Promise<void>((resolve) => window.setTimeout(resolve, delayMs)),
    })
    if (result.kind === 'outdated' || result.kind === 'matched') return result.kind === 'outdated' ? 'outdated' : 'applied'
    if (result.kind === 'exhausted') {
      if (!isRollbackTargetRefreshCurrent(requestId, stackRefreshRequestIdRef.current, latestAppliedStackRefreshRequestIdRef.current)) {
        warnRollbackTargetDiscard('outdated_exhausted_result', requestId, svc, target, 'snapshot')
        return 'outdated'
      }
      setRollbackTarget(null)
      setRollbackActiveTarget(null)
      setRollbackTargetRefreshing(false)
      setError('回滚信息刷新失败，请稍后重试')
      return 'digest_mismatch'
    }
    if (requestId === stackRefreshRequestIdRef.current && requestId >= latestAppliedStackRefreshRequestIdRef.current) {
      setRollbackTarget(null)
      setRollbackActiveTarget(null)
      setRollbackTargetRefreshing(false)
    }
    throw result.error
  }, [applyRollbackTargetSnapshot, serviceId, warnRollbackTargetDiscard])

  const primeRollbackTargetRefresh = useCallback((svc: Service | null) => {
    if (!svc || isDockrevService(svc)) {
      setRollbackTarget(null)
      setRollbackActiveTarget(null)
      setRollbackTargetRefreshing(false)
      return
    }
    const stableRollbackTarget = rollbackTarget ? rollbackTargetMatchesServiceDigest(svc, rollbackTarget) : false
    const stableRollbackActiveTarget = rollbackActiveTarget ? rollbackTargetMatchesServiceDigest(svc, rollbackActiveTarget) : false
    if (stableRollbackActiveTarget && !stableRollbackTarget) {
      setRollbackTarget(null)
    }
    if (!rollbackActiveTarget?.activeJobId) setRollbackActiveTarget(null)
    setRollbackTargetRefreshing(true)
  }, [rollbackActiveTarget, rollbackTarget])

  const refresh = useCallback(async () => {
    const fullRefreshRequestId = ++fullRefreshRequestIdRef.current
    const stackRequestId = ++stackRefreshRequestIdRef.current
    let appliedFullRefreshRoot = false
    setError(null)
    setSettingsPhase(settingsRef.current ? 'refreshing' : 'initial-loading')
    setBackupPhase(backupHasCommittedDataRef.current ? 'refreshing' : 'initial-loading')
    setBackupLoadError(null)
    setRollbackTargetRefreshing(true)
    onLastScanHint?.(undefined)
    try {
      const st = await getStack(stackId)
      const svc = st.services.find((s) => s.id === serviceId) ?? null
      if (stackRequestId === stackRefreshRequestIdRef.current && stackRequestId >= latestAppliedStackRefreshRequestIdRef.current) {
        latestAppliedStackRefreshRequestIdRef.current = stackRequestId
        latestAppliedFullRefreshRequestIdRef.current = fullRefreshRequestId
        appliedFullRefreshRoot = true
        setStack(st)
        setService(svc)
        primeRollbackTargetRefresh(svc)
      }

      const backupResult = Promise.allSettled([
        getServiceBackupTargets(serviceId),
        getServiceBackupRecords(serviceId),
      ]).then(([backupTargetsRes, backupRecordsRes]) => {
        if (
          stackRequestId !== stackRefreshRequestIdRef.current ||
          stackRequestId < latestAppliedStackRefreshRequestIdRef.current ||
          fullRefreshRequestId < latestAppliedFullRefreshRequestIdRef.current
        ) return false
        if (backupTargetsRes.status === 'fulfilled') setBackupTargets(backupTargetsRes.value)
        if (backupRecordsRes.status === 'fulfilled') setBackupRecords(backupRecordsRes.value.records)
        if (backupTargetsRes.status === 'fulfilled' && backupRecordsRes.status === 'fulfilled') {
          backupHasCommittedDataRef.current = true
          setBackupLoaded(true)
          setBackupPhase(backupRecordsRes.value.records.length === 0 ? 'ready-empty' : 'ready-data')
          return true
        }
        const reason = backupTargetsRes.status === 'rejected'
          ? backupTargetsRes.reason
          : backupRecordsRes.status === 'rejected'
            ? backupRecordsRes.reason
            : '服务备份信息暂时不可用，请重试。'
        setBackupLoadError(errorMessage(reason))
        setBackupPhase('error')
        return false
      })
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

      if (
        stackRequestId !== stackRefreshRequestIdRef.current ||
        stackRequestId < latestAppliedStackRefreshRequestIdRef.current ||
        fullRefreshRequestId < latestAppliedFullRefreshRequestIdRef.current
      ) return

      if (settingsRes.status === 'fulfilled') {
        setSettings(settingsRes.value)
        setSettingsPhase('ready-data')
      } else {
        setSettingsPhase('error')
      }
      if (stackSettingsRes.status === 'fulfilled') setStackSettings(stackSettingsRes.value)
      if (rulesRes.status === 'fulfilled') {
        setRules(rulesRes.value.filter((r) => r.scope.serviceId === serviceId))
      }
      if (!svc || isDockrevService(svc)) {
        setRollbackTarget(null); setRollbackActiveTarget(null)
        setRollbackTargetRefreshing(false)
      } else if (rollbackRes.status === 'fulfilled') {
        const rollbackResult = await settleRollbackTargetSnapshot(stackRequestId, svc, rollbackRes.value)
        if (rollbackResult === 'outdated') return
        if (rollbackResult === 'digest_mismatch') throw new Error('回滚信息刷新失败，请稍后重试')
      } else {
        setRollbackTarget(null); setRollbackActiveTarget(null); setRollbackTargetRefreshing(false)
      }
      if (errors.length > 0) throw new Error(errors.join(' · '))
      if (await backupResult) setLastSuccessfulRefreshAt(new Date().toISOString())
    } catch (error: unknown) {
      if (
        stackRequestId !== stackRefreshRequestIdRef.current ||
        stackRequestId < latestAppliedStackRefreshRequestIdRef.current ||
        (appliedFullRefreshRoot && fullRefreshRequestId < latestAppliedFullRefreshRequestIdRef.current)
      ) return
      setRollbackTarget(null)
      setRollbackActiveTarget(null)
      setRollbackTargetRefreshing(false)
      throw error
    }
  }, [onLastScanHint, primeRollbackTargetRefresh, serviceId, settleRollbackTargetSnapshot, stackId])

  const refreshStackOnly = useCallback(async (expectedPageGeneration = pageGenerationRef.current) => {
    if (expectedPageGeneration !== pageGenerationRef.current) return
    const requestId = ++stackRefreshRequestIdRef.current
    setRollbackTargetRefreshing(true)
    try {
      const st = await getStack(stackId)
      if (expectedPageGeneration !== pageGenerationRef.current || requestId !== stackRefreshRequestIdRef.current || requestId < latestAppliedStackRefreshRequestIdRef.current) return
      latestAppliedStackRefreshRequestIdRef.current = requestId
      const svc = st.services.find((s) => s.id === serviceId) ?? null
      setStack(st)
      setService(svc)
      primeRollbackTargetRefresh(svc)
      if (!svc || isDockrevService(svc)) {
        setRollbackTarget(null)
        setRollbackActiveTarget(null)
        setRollbackTargetRefreshing(false)
        return
      }
      const target = await getServiceRollbackTarget(serviceId)
      const rollbackResult = await settleRollbackTargetSnapshot(requestId, svc, target)
      if (rollbackResult === 'digest_mismatch') throw new Error('回滚信息刷新失败，请稍后重试')
    } catch (error: unknown) {
      if (expectedPageGeneration !== pageGenerationRef.current || requestId !== stackRefreshRequestIdRef.current || requestId < latestAppliedStackRefreshRequestIdRef.current) return
      setRollbackTarget(null)
      setRollbackActiveTarget(null)
      setRollbackTargetRefreshing(false)
      throw error
    }
  }, [primeRollbackTargetRefresh, serviceId, settleRollbackTargetSnapshot, stackId])

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

  const requestRefresh = usePageResumeRefresh(refresh, { onError: (error: unknown) => { if (pageGeneration === pageGenerationRef.current) setError(errorMessage(error)) } })

  const refreshLifecycleStatus = useCallback(async (expectedPageGeneration = pageGenerationRef.current) => {
    if (expectedPageGeneration !== pageGenerationRef.current) return
    const requestId = ++lifecycleStatusRequestIdRef.current
    if (!service || isDockrevService(service)) {
      if (requestId === lifecycleStatusRequestIdRef.current) {
        setLifecycleStatus(null)
        lifecycleActiveJobIdRef.current = null
      }
      return
    }
    try {
      const next = await getServiceLifecycleStatus(service.id)
      if (expectedPageGeneration === pageGenerationRef.current && requestId === lifecycleStatusRequestIdRef.current) setLifecycleStatus(next)
    } catch (error: unknown) {
      if (expectedPageGeneration === pageGenerationRef.current && requestId === lifecycleStatusRequestIdRef.current) {
        setLifecycleStatus((previous) => ({
          state: 'unknown',
          unavailableReason: 'lifecycle_status_unavailable',
          activeJob: previous?.activeJob ?? null,
        }))
      }
      throw error
    }
  }, [service])

  const seedLifecycleActiveJob = useCallback(
    (jobId: string, type: 'rollback' | 'service_lifecycle', action: ServiceLifecycleAction | null = null) => {
      lifecycleStatusRequestIdRef.current += 1
      lifecycleActiveJobIdRef.current = jobId
      setLifecycleStatus((previous) => ({
        state: previous?.state ?? 'unknown',
        unavailableReason: previous?.unavailableReason ?? null,
        activeJob: { id: jobId, type, status: 'queued', action },
      }))
    },
    [],
  )

  useEffect(() => {
    const clearSubmittingTokens = () => {
      for (const target of submittingTokensRef.current.values()) {
        endSubmitting(target)
      }
      submittingTokensRef.current.clear()
    }
    clearSubmittingTokens()
    pageGenerationRef.current += 1; lifecycleActiveJobIdRef.current = null
    lifecycleStatusRequestIdRef.current += 1
    setLifecycleSettledJobId(null)
    fullRefreshRequestIdRef.current += 1
    stackRefreshRequestIdRef.current += 1
    backupHasCommittedDataRef.current = false
    setStack(null); setService(null); setSettings(null); setSettingsPhase('initial-loading'); setBackupTargets(null); setBackupRecords([]); setBackupLoaded(false); setBackupLoadError(null); setBackupPhase('initial-loading'); setStackSettings(null); setRules([]); setLifecycleStatus(null); setLastSuccessfulRefreshAt(null)
    setRollbackTarget(null); setRollbackActiveTarget(null); setRollbackTargetRefreshing(false)
    setError(null); setNotice(null); setBusy(false); setRepoInferBusy(false)

    return clearSubmittingTokens
  }, [endSubmitting, serviceId, stackId])

  useEffect(() => {
    const generation = pageGenerationRef.current
    void requestRefresh().catch((e: unknown) => {
      if (generation === pageGenerationRef.current) setError(errorMessage(e))
    })
  }, [requestRefresh, serviceId, stackId])

  useEffect(() => {
    void refreshLifecycleStatus(pageGenerationRef.current).catch(() => {})
  }, [refreshLifecycleStatus])

  useEffect(() => {
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

      const generation = pageGenerationRef.current
      void refreshStackOnly(generation).catch((error: unknown) => { if (generation === pageGenerationRef.current) setError(errorMessage(error)) })
    }

    window.addEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateJobSettled)
    return () => {
      window.removeEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateJobSettled)
    }
  }, [refreshStackOnly, serviceId, stackId])

  useEffect(() => {
    const previousActiveJobId = rollbackActiveJobIdRef.current
    rollbackActiveJobIdRef.current = rollbackActiveJobId
    if (previousActiveJobId && !rollbackActiveJobId) {
      publishServiceTreeRefresh({ stackId, serviceId, reason: 'rollback-job-settled' })
    }
  }, [rollbackActiveJobId, serviceId, stackId])

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

  useManagementEventBatch(({ events, resyncRequired }) => {
    const relevant = resyncRequired || events.some((event) =>
      managementEventAffectsServiceDetail(event, stackId, serviceId, service),
    )
    if (!relevant) return
    const generation = pageGenerationRef.current
    void Promise.all([refreshStackOnly(generation), refreshLifecycleStatus(generation)])
      .then(() => { if (generation === pageGenerationRef.current) publishServiceTreeRefresh({ stackId, serviceId, reason: 'management-event' }) })
      .catch((error: unknown) => { if (generation === pageGenerationRef.current) setError(errorMessage(error)) })
  })

  const archiveOrRestoreService = useCallback(async () => {
    if (!service) return
    const generation = pageGeneration
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
      if (generation === pageGenerationRef.current) setError(errorMessage(e))
    } finally {
      if (generation === pageGenerationRef.current) setBusy(false)
    }
  }, [pageGeneration, requestRefresh, service])

  const blockServiceUpdates = useCallback(async () => {
    const generation = pageGeneration
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
      if (generation === pageGenerationRef.current) setError(errorMessage(e))
    } finally {
      if (generation === pageGenerationRef.current) setBusy(false)
    }
  }, [pageGeneration, requestRefresh, serviceId])

  const requestRollback = useCallback(() => {
    void (async () => {
      const generation = pageGeneration
      if (generation !== pageGenerationRef.current) return
      if (rollbackActiveJobId) {
        navigate({ name: 'job', jobId: rollbackActiveJobId })
        return
      }
      if (!service || !rollbackTarget?.available || !rollbackTarget.targetDigest) return
      const rollbackRequestId = stackRefreshRequestIdRef.current
      const rollbackTargetAtConfirm = rollbackTarget
      const serviceAtConfirm = service
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
              {rollbackBackupValue ? (
                <>
                  <div className="modalKvLabel">来源备份</div>
                  <div className="modalKvValue">
                    <span>{rollbackBackupValue}</span>
                  </div>
                </>
              ) : null}
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
      const latestTarget = rollbackTargetRef.current
      const latestService = serviceRef.current
      if (
        generation !== pageGenerationRef.current ||
        rollbackRequestId !== stackRefreshRequestIdRef.current ||
        rollbackTargetRefreshingRef.current ||
        latestTarget !== rollbackTargetAtConfirm ||
        !latestService ||
        latestService.id !== serviceAtConfirm.id ||
        !rollbackTargetMatchesServiceDigest(latestService, latestTarget) ||
        !latestTarget?.available ||
        !latestTarget.targetDigest
      ) {
        if (generation !== pageGenerationRef.current) return
        setError('回滚信息已更新，请重新确认')
        return
      }
      setBusy(true)
      setError(null)
      setNotice(null)
      try {
        const resp = await triggerServiceRollback(service.id)
        if (generation !== pageGenerationRef.current) return
        seedLifecycleActiveJob(resp.jobId, 'rollback')
        setNotice({ jobId: resp.jobId, kind: 'rollback' })
        publishServiceTreeRefresh({ stackId, serviceId, reason: 'rollback-job-started' })
        await refreshStackOnly(generation)
        await refreshLifecycleStatus(generation)
      } catch (e: unknown) {
        if (generation !== pageGenerationRef.current) return
        if (e instanceof ApiError) {
          if (e.status === 401) setError('需要登录/鉴权（Forward Auth）')
          else if (e.status === 409) {
            const details = e.details
            const existingJobId = conflictingJobId(e)
            const reason =
              details && typeof details === 'object' && details !== null && 'reason' in details
                ? (details as Record<string, unknown>).reason
                : null
            if (existingJobId) {
              navigate({ name: 'job', jobId: existingJobId })
            } else if (typeof reason === 'string' && reason.trim()) {
              setError(rollbackUnavailableReasonLabel(reason) ?? e.message)
            } else {
              setError(e.message)
            }
            await refreshStackOnly(generation)
          } else setError(e.message)
        } else {
          setError(errorMessage(e))
        }
      } finally {
        if (generation === pageGenerationRef.current) setBusy(false)
      }
    })()
  }, [confirm, pageGeneration, refreshLifecycleStatus, refreshStackOnly, rollbackActiveJobId, rollbackBackupValue, rollbackTarget, seedLifecycleActiveJob, service, serviceId, stack?.name, stackId])

  const requestLifecycleAction = useCallback((action: ServiceLifecycleAction) => {
    void (async () => {
      const generation = pageGeneration
      if (generation !== pageGenerationRef.current) return
      if (!service) return
      const activeJobId = activeLifecycleJob?.id
      if (activeJobId) {
        navigate({ name: 'job', jobId: activeJobId })
        return
      }
      if (action !== 'start') {
        const actionLabel = action === 'stop' ? '停止' : '重启'
        const ok = await confirm({
          title: `确认${actionLabel}服务 ${service.name}？`,
          body: <div className="modalLead">该操作会立即影响服务运行状态。</div>,
          confirmText: actionLabel,
          cancelText: '取消',
          confirmVariant: action === 'stop' ? 'danger' : 'primary',
          badgeText: null,
        })
        if (!ok) return
      }
      if (generation !== pageGenerationRef.current) return
      setBusy(true)
      setError(null)
      setNotice(null)
      try {
        const resp = await triggerServiceLifecycle(service.id, action)
        if (generation !== pageGenerationRef.current) return
        seedLifecycleActiveJob(resp.jobId, 'service_lifecycle', action)
        setNotice({ jobId: resp.jobId, kind: 'lifecycle' })
        publishServiceTreeRefresh({ stackId, serviceId, reason: 'lifecycle-job-started' })
        await refreshLifecycleStatus(generation)
      } catch (e: unknown) {
        if (generation !== pageGenerationRef.current) return
        if (e instanceof ApiError && e.status === 409) {
          const existingJobId = conflictingJobId(e)
          if (existingJobId) {
            navigate({ name: 'job', jobId: existingJobId })
          } else {
            setError(e.message)
          }
          await refreshLifecycleStatus(generation).catch(() => undefined)
        } else {
          setError(errorMessage(e))
        }
      } finally {
        if (generation === pageGenerationRef.current) setBusy(false)
      }
    })()
  }, [activeLifecycleJob?.id, confirm, pageGeneration, refreshLifecycleStatus, seedLifecycleActiveJob, service, serviceId, stackId])

  const requestPreviewUpdate = useCallback(() => {
    void (async () => {
      const generation = pageGeneration
      if (generation !== pageGenerationRef.current || !service || !service.candidate) return
      setBusy(true)
      setError(null)
      setNotice(null)
      try {
        const updateTarget = await buildUpdateServiceTarget(service)
        if (generation !== pageGenerationRef.current) return
        const resp = await triggerUpdate({
          scope: 'service',
          stackId,
          ...updateTarget,
          mode: 'dry-run',
          allowArchMismatch: false,
          backupMode: 'inherit',
        })
        if (generation !== pageGenerationRef.current) return
        setNotice({ jobId: resp.jobId, kind: 'update' })
      } catch (e: unknown) {
        if (generation !== pageGenerationRef.current) return
        if (e instanceof ApiError && e.status === 401) setError('需要登录/鉴权（Forward Auth）')
        else if (e instanceof ApiError && e.status === 409) {
          setError('扫描结果已变化，请刷新并重新扫描后再更新')
          await requestRefresh()
        } else setError(errorMessage(e))
      } finally {
        if (generation === pageGenerationRef.current) setBusy(false)
      }
    })()
  }, [pageGeneration, requestRefresh, service, stackId])

  const requestApplyUpdate = useCallback(() => {
    void (async () => {
      const generation = pageGeneration
      if (generation !== pageGenerationRef.current || !service || !service.candidate) return
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
        confirmText: '更新',
        cancelText: '取消',
        confirmVariant: 'primary',
        badgeText: null,
      })
      if (!ok || generation !== pageGenerationRef.current) return
      setError(null)
      setNotice(null)
      let submissionToken: symbol | null = null
      if (applyActionKey) {
        submissionToken = Symbol(applyActionKey)
        submittingTokensRef.current.set(submissionToken, applyActionKey)
        beginSubmitting(applyActionKey)
      }
      try {
        const updateTarget = await buildUpdateServiceTarget(service)
        if (generation !== pageGenerationRef.current) return
        const resp = await triggerUpdate({
          scope: 'service',
          stackId,
          ...updateTarget,
          mode: 'apply',
          allowArchMismatch: false,
          backupMode: 'inherit',
        })
        if (generation !== pageGenerationRef.current) return
        setNotice({ jobId: resp.jobId, kind: 'update' })
        if (applyActionKey) trackJob(applyActionKey, resp.jobId, 'queued', service.candidate?.resolvedTag ?? service.candidate?.tag ?? null)
        void refreshLifecycleStatus(generation).catch(() => undefined)
      } catch (e: unknown) {
        if (generation !== pageGenerationRef.current) return
        if (e instanceof ApiError) {
          if (e.status === 401) setError('需要登录/鉴权（Forward Auth）')
          else if (e.status === 409) {
            const existingJobId = conflictingJobId(e)
            if (existingJobId) {
              navigate({ name: 'job', jobId: existingJobId })
            } else {
              const guardReason = readUpdateGuardBlockedReason(e)
              if (guardReason) {
                setError(guardReason)
                return
              }
              setError('扫描结果已变化，请刷新并重新扫描后再更新')
              await requestRefresh()
            }
          } else setError(e.message)
        } else {
          setError(errorMessage(e))
        }
      } finally {
        if (submissionToken) {
          const target = submittingTokensRef.current.get(submissionToken)
          if (target) {
            submittingTokensRef.current.delete(submissionToken)
            endSubmitting(target)
          }
        }
      }
    })()
  }, [
    applyActionKey,
    beginSubmitting,
    confirm,
    endSubmitting,
    patchServiceInStack,
    pageGeneration,
    refreshLifecycleStatus,
    requestRefresh,
    service,
    stackId,
    trackJob,
  ])

  const openDockrevSelfUpgrade = useCallback(() => {
    openSelfUpgradeUrl(selfUpgradeUrl)
  }, [selfUpgradeUrl])

  const retryDockrevSelfUpgrade = useCallback(() => {
    void checkSupervisor()
  }, [checkSupervisor])

  const dockrevSelfUpgradeAction = useMemo(() => {
    if (!service || !isDockrevService(service)) return null
    const disabledReason =
      supervisorState.status === 'offline'
        ? `自我升级不可用（supervisor offline） · ${supervisorErrorAt ?? '-'} · ${supervisorError ?? '-'}`
        : supervisorState.status === 'checking'
          ? '检查 supervisor 中…'
          : busy
            ? dockrevSelfUpgradeBusyReason()
          : null
    return {
      label: '升级 Dockrev',
      disabled: busy || supervisorState.status !== 'ok',
      disabledReason,
      status: supervisorState.status,
      retryVisible: supervisorState.status !== 'ok',
      retryDisabled: busy || supervisorState.status === 'checking',
      open: openDockrevSelfUpgrade,
      retry: retryDockrevSelfUpgrade,
    } as const
  }, [
    busy,
    openDockrevSelfUpgrade,
    retryDockrevSelfUpgrade,
    service,
    supervisorError,
    supervisorErrorAt,
    supervisorState.status,
  ])

  const topActions = useMemo(() => {
    if (dockrevSelfUpgradeAction) {
      const selfUpgradeItems = [
        { id: 'dockrev-upgrade', label: dockrevSelfUpgradeAction.label, icon: ArrowUpCircle, description: dockrevSelfUpgradeAction.disabledReason ?? undefined, disabled: dockrevSelfUpgradeAction.disabled, onSelect: dockrevSelfUpgradeAction.open },
        ...(dockrevSelfUpgradeAction.retryVisible ? [{ id: 'dockrev-upgrade-retry', label: '重试', icon: RotateCw, disabled: dockrevSelfUpgradeAction.retryDisabled, onSelect: dockrevSelfUpgradeAction.retry }] : []),
      ]
      const stackItem = { id: 'stack-details', label: 'Stack 详情', icon: Layers3, disabled: busy, onSelect: () => navigate({ name: 'stack' as const, stackId }) }
      return (
        <>
          <div className="serviceDesktopActions">
            <Button variant="primary" disabled={dockrevSelfUpgradeAction.disabled} hint={dockrevSelfUpgradeAction.disabledReason ?? undefined} onClick={dockrevSelfUpgradeAction.open}>
              {dockrevSelfUpgradeAction.label}
            </Button>
            {dockrevSelfUpgradeAction.retryVisible ? <Button variant="ghost" disabled={dockrevSelfUpgradeAction.retryDisabled} onClick={dockrevSelfUpgradeAction.retry}>重试</Button> : null}
            <ServiceStackDetailAction disabled={busy} onClick={() => navigate({ name: 'stack', stackId })} />
          </div>
          <ServiceMobileActionMenu groups={[{ id: 'upgrade', items: selfUpgradeItems }, { id: 'stack', items: [stackItem] }]} />
        </>
      )
    }

    const candidateReason = !service
      ? '服务信息加载中'
      : service.ignore?.matched
        ? service.ignore.reason ?? '被阻止'
        : serviceRowStatus(service) === 'blocked'
          ? blockedReasonFor(service) ?? '被阻止'
        : !service.candidate
          ? '无候选版本'
          : service.candidate.archMatch === 'mismatch'
            ? '架构不匹配（仅提示，不允许更新）'
            : undefined
    const previewDisabled = busy || Boolean(candidateReason)
    const applyDisabled = applySubmitting && !applyActiveJob
      ? true
      : !applyActiveJob && (busy || Boolean(candidateReason))
    const applyDescription = activeUpdateJob
      ? '任务进行中，点击查看任务详情'
      : applySubmitting
        ? '正在提交更新任务'
        : candidateReason
    const rollbackLabel = activeRollbackJob
      ? rollbackActiveJobStatus === 'queued' ? '回滚排队中…' : '回滚中…'
      : rollbackTargetRefreshing ? '回滚信息刷新中…' : '回滚'
    const rollbackDisabled = !activeRollbackJob && (busy || rollbackTargetRefreshing || !rollbackTarget?.available)
    const operationBusyReason = activeOperationOwner === 'update'
      ? '服务正在更新，完成后才能启动、停止或重启。'
      : activeOperationOwner === 'rollback'
        ? '服务正在回滚，完成后才能启动、停止或重启。'
        : activeOperationOwner === 'lifecycle'
          ? `服务正在${activeOperation?.action === 'start' ? '启动' : activeOperation?.action === 'stop' ? '停止' : '重启'}，完成后才能更新或回滚。`
          : undefined
    const updateGroupDisabledReason = activeOperationOwner === 'lifecycle' ? operationBusyReason : undefined
    const lifecycleGroupDisabledReason = activeOperationOwner === 'update' || activeOperationOwner === 'rollback' ? operationBusyReason : undefined
    const updateItems = [
      {
        id: 'preview-update',
        label: '预览更新',
        icon: Eye,
        description: activeOperationOwner ? operationBusyReason : candidateReason,
        disabled: Boolean(activeOperationOwner) || previewDisabled,
        onSelect: requestPreviewUpdate,
      },
      {
        id: 'execute-update',
        label: activeUpdateJob ? (activeUpdateJob.status === 'queued' ? '更新排队中…' : '更新中…') : '更新',
        icon: Download,
        description: activeUpdateJob ? '任务进行中，点击查看任务详情' : activeOperationOwner ? operationBusyReason : applyDescription,
        disabled: activeUpdateJob ? false : Boolean(activeOperationOwner) || applyDisabled,
        loading: Boolean(activeUpdateJob || applySubmitting),
        loadingClickable: Boolean(activeUpdateJob),
        onSelect: () => activeUpdateJob ? navigate({ name: 'job', jobId: activeUpdateJob.id }) : requestApplyUpdate(),
      },
      {
        id: 'rollback',
        label: rollbackLabel,
        icon: RotateCcw,
        description: activeRollbackJob ? '任务进行中，点击查看任务详情' : activeOperationOwner ? operationBusyReason : rollbackHint,
        disabled: activeRollbackJob ? false : Boolean(activeOperationOwner) || rollbackDisabled,
        loading: Boolean(activeRollbackJob),
        loadingClickable: Boolean(activeRollbackJob),
        onSelect: requestRollback,
      },
    ]
    const updatePrimary = activeUpdateJob || activeRollbackJob ? (activeUpdateJob ? updateItems[1] : updateItems[2]) : service?.candidate ? updateItems[1] : updateItems[2]
    const lifecycleState = lifecycleStatus?.state ?? 'unknown'
    const lifecycleReason = lifecycleStatus?.unavailableReason === 'partial_replicas_running'
      ? '部分副本正在运行，请先处理运行态异常'
      : lifecycleStatus?.unavailableReason === 'lifecycle_status_unavailable'
        ? '无法读取服务运行状态，请刷新后重试'
        : lifecycleStatus?.unavailableReason
    const lifecycleItem = (action: ServiceLifecycleAction, label: string) => {
      const compatible = (action === 'start' && lifecycleState === 'stopped') || ((action === 'stop' || action === 'restart') && lifecycleState === 'running')
      const isActiveAction = Boolean(activeLifecycleJob && activeLifecycleJob.action === action)
      const description = activeLifecycleJob
        ? isActiveAction ? '任务进行中，点击查看任务详情' : '其他生命周期任务进行中'
        : activeOperationOwner ? operationBusyReason : lifecycleReason ?? (compatible ? undefined : '当前服务状态不支持该操作')
      const icon = action === 'start' ? Play : action === 'stop' ? Square : RotateCw
      return {
        id: `lifecycle-${action}`,
        label,
        icon,
        iconVariant: action === 'start' || action === 'stop' ? 'solid' as const : undefined,
        description,
        disabled: activeLifecycleJob ? !isActiveAction : Boolean(activeOperationOwner) || busy || !compatible,
        onSelect: () => activeLifecycleJob && isActiveAction ? navigate({ name: 'job', jobId: activeLifecycleJob.id }) : requestLifecycleAction(action),
      }
    }
    const lifecycleItems = [lifecycleItem('start', '启动'), lifecycleItem('stop', '停止'), lifecycleItem('restart', '重启')]
    const lifecyclePrimary = activeLifecycleJob
      ? { ...lifecycleItems.find((item) => item.id === `lifecycle-${activeLifecycleJob.action ?? 'restart'}`)!, label: activeLifecycleJob.status === 'queued' ? '操作排队中…' : '操作进行中…', disabled: false, loading: true, loadingClickable: true, description: '任务进行中，点击查看任务详情' }
      : lifecycleState === 'stopped' ? lifecycleItems[0] : lifecycleItems[1]
    const stackItem = { id: 'stack-details', label: 'Stack 详情', icon: Layers3, disabled: busy, onSelect: () => navigate({ name: 'stack' as const, stackId }) }

    return (
      <>
        <div className="serviceDesktopActions">
          <ServiceSplitActionButton ariaLabel="更新操作" disabled={Boolean(updateGroupDisabledReason)} disabledReason={updateGroupDisabledReason} items={updateItems} primary={updatePrimary} />
          <ServiceSplitActionButton ariaLabel="服务生命周期" disabled={Boolean(lifecycleGroupDisabledReason)} disabledReason={lifecycleGroupDisabledReason} items={lifecycleItems} primary={lifecyclePrimary} />
          <ServiceStackDetailAction disabled={busy} onClick={() => navigate({ name: 'stack', stackId })} />
        </div>
        <ServiceMobileActionMenu groups={[
          { id: 'update', items: updateItems },
          { id: 'lifecycle', items: lifecycleItems },
          { id: 'stack', items: [stackItem] },
        ]} />
      </>
    )
  }, [activeLifecycleJob, activeOperation, activeOperationOwner, activeRollbackJob, activeUpdateJob, applyActiveJob, applySubmitting, busy, dockrevSelfUpgradeAction, lifecycleStatus, requestApplyUpdate, requestLifecycleAction, requestPreviewUpdate, requestRollback, rollbackActiveJobStatus, rollbackHint, rollbackTarget?.available, rollbackTargetRefreshing, service, stackId])

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

  const draftRepoUrl = useMemo(() => normalizeExternalHttpUrl(settings?.repoUrl), [settings?.repoUrl])
  const settingsBusy = busy || repoInferBusy

  const tone = useMemo(() => (service ? svcTone(service) : 'muted'), [service])
  const bannerClass = operationProgress
    ? 'svcBanner svcBannerInfo'
    : tone === 'ok' ? 'svcBanner svcBannerOk' : tone === 'warn' ? 'svcBanner svcBannerWarn' : tone === 'bad' ? 'svcBanner svcBannerBad' : 'svcBanner svcBannerMuted'
  const dotClass =
    tone === 'ok'
      ? 'svcBannerDot svcBannerDotOk'
      : tone === 'warn'
        ? 'svcBannerDot svcBannerDotWarn'
        : tone === 'bad'
          ? 'svcBannerDot svcBannerDotBad'
          : 'svcBannerDot'

  const bannerTitle = useMemo(() => {
    if (operationProgress) return operationProgress.bannerLabel
    if (!service) return '加载中…'
    const st = serviceRowStatus(service)
    if (st === 'blocked') {
      return '已阻止（忽略规则命中）'
    }
    if (st === 'ok') return '暂无候选版本'
    if (st === 'archMismatch') return '架构不匹配（仅提示，不允许更新）'
    if (st === 'hint') return '需确认（arch 未知）'
    return '可更新'
  }, [operationProgress, service])

  const bannerDetail = useMemo<ReactNode>(() => {
    if (!service) return null

    const currentTag = formatTagDisplay(
      service.image.tag,
      service.image.resolvedTag,
      service.versionInference?.status,
    )
    const candidateTag = service.candidate
      ? formatCandidateTagDisplay(
      service.candidate.tag,
      service.candidate.resolvedTag ?? null,
      service.versionInference?.status,
    )
      : '-'
    const discoveryCount = service.newVersionDiscoveryCount
    const versionSpan =
      typeof discoveryCount === 'number' ? `跨 ${discoveryCount} 个版本` : '跨度未知'
    return (
      <span className="svcBannerSummary">
        <span>
          当前 <Mono>{currentTag || '-'}</Mono>
        </span>
        <span>
          目标 <Mono>{candidateTag || '-'}</Mono>
        </span>
        <span>{versionSpan}</span>
      </span>
    )
  }, [service])

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
    backupRecords,
    backupPhase,
    backupLoaded,
    backupLoadError,
    backupTargets,
    busy,
    composeEnvFile,
    composeFiles,
    composeType,
    dotClass,
    draftRepoUrl,
    dockrevSelfUpgradeAction,
    error,
    lastSuccessfulRefreshAt,
    lifecycleSettledJobId,
    newRuleKind,
    newRuleNote,
    newRuleValue,
    notice,
    operationProgress,
    repoInferBusy,
    requestRefresh,
    requestApplyUpdate,
    requestRollback,
    rollbackTarget,
    rollbackActiveJobId,
    rollbackActiveJobStatus,
    rollbackTargetRefreshing,
    rules,
    semverDowngradeAnomaly,
    service,
    serviceId,
    applyActiveJob: activeUpdateJob
      ? { jobId: activeUpdateJob.id, status: activeUpdateJob.status, targetVersion: activeUpdateJob.targetVersion }
      : null,
    applySubmitting,
    setBusy,
    setError,
    setNewRuleKind,
    setNewRuleNote,
    setNewRuleValue,
    setRepoInferBusy,
    setSettings,
    settings,
    settingsPhase,
    stackSettings,
    settingsBusy,
    stack,
    supervisorErrorAt,
    supervisorState,
    topActions,
    tone,
    dangerousActions,
  }
}
