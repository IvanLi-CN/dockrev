import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import {
  archiveService,
  ApiError,
  createIgnore,
  deleteIgnore,
  getServiceSettings,
  getStack,
  listIgnores,
  newJobEventsSource,
  putServiceSettings,
  restoreService,
  triggerRuntimeScan,
  triggerUpdate,
  type IgnoreRule,
  type Service,
  type ServiceSettings,
  type StackDetail,
} from '../api'
import { navigate } from '../routes'
import { ArrowRightIcon, Button, Mono, Pill, Switch } from '../ui'
import { isDockrevImageRef, selfUpgradeBaseUrl } from '../runtimeConfig'
import { useSupervisorHealth } from '../useSupervisorHealth'
import { serviceRowStatus } from '../updateStatus'
import { CurrentVersionPopover } from '../components/CurrentVersionPopover'
import { VersionTagsPopover } from '../components/VersionTagsPopover'
import { useConfirm } from '../confirm'
import { formatCandidateTagDisplay, formatCurrentTagDisplay as formatTagDisplay, isStrictSemverTag } from '../versionDisplay'

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function svcTone(svc: Service): 'ok' | 'warn' | 'bad' | 'muted' {
  const st = serviceRowStatus(svc)
  if (st === 'updatable') return 'ok'
  if (st === 'hint') return 'warn'
  if (st === 'archMismatch' || st === 'blocked') return 'bad'
  return 'muted'
}

function svcBadge(svc: Service): string {
  const st = serviceRowStatus(svc)
  if (st === 'blocked') return '被阻止'
  if (st === 'archMismatch') return '架构不匹配'
  if (st === 'hint') return '需确认'
  if (st === 'updatable') return '可更新'
  return '无候选'
}

function formatMap(map: Record<string, string>) {
  const keys = Object.keys(map)
  if (keys.length === 0) return []
  return keys.map((k) => ({ key: k, value: map[k] }))
}

function shortDigest(digest: string) {
  if (digest.length <= 24) return digest
  return `${digest.slice(0, 12)}…${digest.slice(-8)}`
}

function splitImageRef(ref: string): { registry: string; name: string } {
  const s = ref.trim()
  const withoutDigest = s.includes('@') ? s.split('@', 1)[0] : s
  const firstSlash = withoutDigest.indexOf('/')
  if (firstSlash < 0) {
    return { registry: 'docker.io', name: withoutDigest }
  }
  const firstSeg = withoutDigest.slice(0, firstSlash)
  const rest = withoutDigest.slice(firstSlash + 1)
  const isRegistry = firstSeg.includes('.') || firstSeg.includes(':') || firstSeg === 'localhost'
  if (isRegistry) return { registry: firstSeg, name: rest }
  return { registry: 'docker.io', name: withoutDigest }
}

function splitImageNameForDisplay(
  name: string,
  tag: string | null | undefined,
): { base: string; suffix: string } {
  const n = name.trim()
  if (!n) return { base: '', suffix: '' }

  const at = n.indexOf('@')
  if (at >= 0) return { base: n.slice(0, at), suffix: n.slice(at) }

  const lastSlash = n.lastIndexOf('/')
  const lastColon = n.lastIndexOf(':')
  if (lastColon > lastSlash) return { base: n.slice(0, lastColon), suffix: n.slice(lastColon) }

  const t = (tag ?? '').trim()
  if (!t) return { base: n, suffix: '' }
  if (t.startsWith('sha256:')) return { base: n, suffix: `@${t}` }
  return { base: n, suffix: `:${t}` }
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

export function ServiceDetailPage(props: {
  stackId: string
  serviceId: string
  onLastScanHint?: (lastScan?: string) => void
  onTopActions: (node: React.ReactNode) => void
}) {
  const { stackId, serviceId, onLastScanHint, onTopActions } = props
  const confirm = useConfirm()
  const [stack, setStack] = useState<StackDetail | null>(null)
  const [service, setService] = useState<Service | null>(null)
  const [settings, setSettings] = useState<ServiceSettings | null>(null)
  const [rules, setRules] = useState<IgnoreRule[]>([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [noticeJobId, setNoticeJobId] = useState<string | null>(null)
  const { state: supervisorState, check: checkSupervisor } = useSupervisorHealth()
  const supervisorErrorAt = supervisorState.status === 'offline' ? supervisorState.errorAt : undefined
  const supervisorError = supervisorState.status === 'offline' ? supervisorState.error : undefined
  const selfUpgradeUrl = useMemo(() => selfUpgradeBaseUrl(), [])

  const [newRuleKind, setNewRuleKind] = useState<'exact' | 'prefix' | 'regex' | 'semver'>('regex')
  const [newRuleValue, setNewRuleValue] = useState('.*')
  const [newRuleNote, setNewRuleNote] = useState('blocked via UI')

  const refresh = useCallback(async () => {
    setError(null)
    onLastScanHint?.(undefined)
    const st = await getStack(stackId)
    setStack(st)
    const svc = st.services.find((s) => s.id === serviceId) ?? null
    setService(svc)
    setSettings(await getServiceSettings(serviceId))
    const allRules = await listIgnores()
    setRules(allRules.filter((r) => r.scope.serviceId === serviceId))
  }, [onLastScanHint, serviceId, stackId])

  const refreshStackOnly = useCallback(async () => {
    const st = await getStack(stackId)
    setStack(st)
    const svc = st.services.find((s) => s.id === serviceId) ?? null
    setService(svc)
  }, [serviceId, stackId])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(errorMessage(e)))
  }, [refresh])

  useEffect(() => {
    const onVersionRefresh = () => {
      void refreshStackOnly().catch(() => {})
    }
    window.addEventListener('dockrev:version-inference-refresh', onVersionRefresh)
    return () => {
      window.removeEventListener('dockrev:version-inference-refresh', onVersionRefresh)
    }
  }, [refreshStackOnly])

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

  useEffect(() => {
    onTopActions(
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
                  setNoticeJobId(null)
                  try {
                    const resp = await triggerUpdate({
                      scope: 'service',
                      stackId,
                      serviceId,
                      targetDigest: service.candidate.digest,
                      mode: 'dry-run',
                      allowArchMismatch: false,
                      backupMode: 'inherit',
                    })
                    setNoticeJobId(resp.jobId)
                  } catch (e: unknown) {
                    if (e instanceof ApiError) {
                      if (e.status === 401) setError('需要登录/鉴权（forward header）')
                      else if (e.status === 409) {
                        setError('扫描结果已变化，请刷新并重新扫描后再更新')
                        await refresh()
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
	                    const currentDisplayTag = formatTagDisplay(
	                      service.image.tag,
	                      service.image.resolvedTag,
	                      service.versionInference?.status,
	                    )
	                    const inferencePending = service.versionInference?.status === 'pending'
                      const candidatePrefetchOnMount = shouldPrefetchFloatingCandidate(
                        service.candidate.tag,
                        service.candidate.resolvedTag ?? null,
                        service.candidate.digest ?? null,
                      )
	                    const candidateDisplayTag = formatCandidateTagDisplay(
	                      service.candidate.tag,
	                      service.candidate.resolvedTag ?? null,
	                      service.versionInference?.status,
	                    )
	                    const rawTagTrim = (service.image.tag ?? '').trim()
                    const showRawTag = Boolean(rawTagTrim && rawTagTrim !== currentDisplayTag)
	                  const ok = await confirm({
	                    title: `确认更新服务 ${service.name}？`,
	                    body: (
	                      <>
	                        <div className="modalLead">将对该服务执行更新（apply）。</div>
	                        <div className="modalKvGrid">
	                          <div className="modalKvLabel">范围</div>
	                          <div className="modalKvValue">
	                            <Mono>service</Mono>
	                          </div>
	                          <div className="modalKvLabel">目标</div>
	                          <div className="modalKvValue">
	                            <Mono>{`${stack?.name ?? stackId}/${service.name}`}</Mono>
	                          </div>
		                          <div className="modalKvLabel">镜像</div>
		                          <div className="modalKvValue">
		                            {(() => {
		                              const img = splitImageRef(service.image.ref)
		                              const dn = splitImageNameForDisplay(img.name, service.image.tag)
		                              return (
		                                <div className="cellTwoLine">
		                                  <div
		                                    className="mono monoPrimary monoSplit"
		                                    title={dn.suffix ? `${dn.base}${dn.suffix}` : dn.base}
		                                  >
		                                    <span className="monoSplitBase">{dn.base}</span>
		                                  </div>
		                                  <div className="mono monoSecondary">{img.registry}</div>
		                                </div>
		                              )
		                            })()}
		                          </div>
		                          <div className="modalKvLabel">目标版本</div>
		                          <div className="modalKvValue">
                                <div className="cellTwoLine">
                                <div className="versionLine">
                                  <CurrentVersionPopover
                                    serviceId={service.id}
                                    displayTag={currentDisplayTag}
                                    imageTag={service.image.tag}
                                    imageDigest={service.image.digest ?? null}
                                    resolvedTag={service.image.resolvedTag}
                                    resolvedTags={service.image.resolvedTags}
                                    inferenceLoading={inferencePending}
	                                  />
	                                  <span
	                                    className={inferencePending ? 'inlineIconLoading' : 'inlineIconMuted'}
	                                    style={inferencePending ? { margin: '0 6px' } : { opacity: 0.8, margin: '0 6px' }}
	                                  >
	                                    <ArrowRightIcon className="inlineIcon" />
	                                  </span>
	                                  <VersionTagsPopover
	                                    serviceId={service.id}
	                                    candidateTag={service.candidate.tag}
	                                    candidateDigest={service.candidate.digest ?? null}
                                      prefetchOnMount={candidatePrefetchOnMount}
	                                  >
	                                    {candidateDisplayTag}
	                                  </VersionTagsPopover>
	                                </div>
	                                {showRawTag ? (
                                  <div>
                                    <CurrentVersionPopover
                                      serviceId={service.id}
                                      displayTag={service.image.tag}
                                      imageTag={service.image.tag}
                                      imageDigest={service.image.digest ?? null}
                                      resolvedTag={service.image.resolvedTag}
                                      resolvedTags={service.image.resolvedTags}
                                      preferSource="rawTag"
                                      triggerClassName="versionTagsTrigger mono monoSecondary"
                                    >
                                      {service.image.tag}
                                    </CurrentVersionPopover>
                                  </div>
                                ) : null}
                              </div>
		                          </div>
	                          <div className="modalKvLabel">状态</div>
	                          <div className="modalKvValue">
	                            <Mono>{serviceRowStatus(service)}</Mono>
	                          </div>
	                          <div className="modalKvLabel">备份</div>
	                          <div className="modalKvValue">
	                            <Mono>inherit</Mono>
	                          </div>
	                          <div className="modalKvLabel">架构不匹配</div>
	                          <div className="modalKvValue">
	                            <Mono>disallow</Mono>
	                          </div>
	                        </div>
	                        <div className="modalDivider" />
	                      </>
	                    ),
	                    confirmText: '执行更新',
	                    cancelText: '取消',
	                    confirmVariant: 'primary',
                      // Hide the pill badge; the intent is already visible in the modal body.
                      badgeText: null,
	                  })
                  if (!ok) return
                  setBusy(true)
                  setError(null)
                  setNoticeJobId(null)
                  try {
                    const resp = await triggerUpdate({
                      scope: 'service',
                      stackId,
                      serviceId,
                      targetDigest: service.candidate.digest,
                      mode: 'apply',
                      allowArchMismatch: false,
                      backupMode: 'inherit',
                    })
                    setNoticeJobId(resp.jobId)
                  } catch (e: unknown) {
                    if (e instanceof ApiError) {
                      if (e.status === 401) setError('需要登录/鉴权（forward header）')
                      else if (e.status === 409) {
                        setError('扫描结果已变化，请刷新并重新扫描后再更新')
                        await refresh()
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
              执行更新
            </Button>
          </>
        )}
        <Button
          variant={service?.archived ? 'primary' : 'ghost'}
          disabled={busy || !service}
          onClick={() => {
            void (async () => {
              if (!service) return
              setBusy(true)
              setError(null)
              try {
                if (service.archived) {
                  await restoreService(service.id)
                } else {
                  await archiveService(service.id)
                }
                await refresh()
              } catch (e: unknown) {
                setError(errorMessage(e))
              } finally {
                setBusy(false)
              }
            })()
          }}
        >
          {service?.archived ? '恢复' : '归档'}
        </Button>
        <Button
          variant="danger"
          disabled={busy}
          onClick={() => {
            void (async () => {
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
                await refresh()
              } catch (e: unknown) {
                setError(errorMessage(e))
              } finally {
                setBusy(false)
              }
            })()
          }}
        >
          阻止此服务更新
        </Button>
      </>,
    )
  }, [
    busy,
    checkSupervisor,
    confirm,
    onTopActions,
    refresh,
    selfUpgradeUrl,
    service,
    serviceId,
    stackId,
    stack?.name,
    supervisorErrorAt,
    supervisorError,
    supervisorState.status,
  ])

  const bindTargets = useMemo(() => (settings ? formatMap(settings.backupTargets.bindPaths) : []), [settings])
  const volTargets = useMemo(() => (settings ? formatMap(settings.backupTargets.volumeNames) : []), [settings])

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
    if (st === 'blocked') return '已阻止（忽略规则命中）'
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
	        >
	          {candidateDisplayTag}
	        </VersionTagsPopover>
	        <span className="mono">{`@${shortDigest(service.candidate.digest)}`}</span>
	        {archNode}
	      </>
    )
  }, [service])

  if (!stack || !service || !settings) {
    return <div className="muted">加载中…</div>
  }

  const rawComposeType = typeof stack.compose?.type === 'string' ? stack.compose.type.trim() : ''
  const composeType = rawComposeType || '-'
  const composeFilesRaw = Array.isArray(stack.compose?.composeFiles) ? stack.compose.composeFiles : []
  const composeFiles = composeFilesRaw
    .map((item) => (typeof item === 'string' ? item.trim() : ''))
    .filter((item) => item.length > 0)
  const composeEnvFileRaw = typeof stack.compose?.envFile === 'string' ? stack.compose.envFile.trim() : ''
  const composeEnvFile = composeEnvFileRaw || '-'

  return (
    <div className="page">
      <div className="svcTitleRow">
        <div className="svcTitleMain">
          <div className="svcTitleNameRow">
            <div className="svcTitleName">
              服务: <Mono>{service.name}</Mono>
            </div>
            <Pill tone="muted">{stack.name}</Pill>
          </div>
          {(() => {
            const img = splitImageRef(service.image.ref)
            const dn = splitImageNameForDisplay(img.name, service.image.tag)
            return (
              <div className="cellTwoLine">
                <div
                  className="mono monoPrimary monoSplit"
                  title={dn.suffix ? `${dn.base}${dn.suffix}` : dn.base}
                >
                  <span className="monoSplitBase">{dn.base}</span>
                </div>
                <div className="mono monoSecondary">{img.registry}</div>
              </div>
            )
          })()}
          <div className="muted">
            id <Mono>{service.id}</Mono> · stack <Mono>{stack.id}</Mono>
          </div>
        </div>
      </div>

      <div className="card svcComposeCard">
        <div className="title">Compose 信息</div>
        <div className="kv">
          <div className="kvRow">
            <div className="muted">type</div>
            <div className="mono">{composeType}</div>
          </div>
          <div className="kvRow">
            <div className="muted">compose files</div>
            {composeFiles.length > 0 ? (
              <div>
                {composeFiles.map((item, index) => (
                  <div key={`${item}-${index}`} className="mono">
                    {item}
                  </div>
                ))}
              </div>
            ) : (
              <div className="mono">-</div>
            )}
          </div>
          <div className="kvRow">
            <div className="muted">env file</div>
            <div className="mono">{composeEnvFile}</div>
          </div>
        </div>
      </div>

      <div className={bannerClass}>
        <div className="svcBannerTitleRow">
          <span className={dotClass} />
          <div className="svcBannerTitle">{bannerTitle}</div>
          <div style={{ marginLeft: 'auto' }}>
            <Pill tone={tone}>{svcBadge(service)}</Pill>
          </div>
        </div>
        <div className="svcBannerDetail">{bannerDetail}</div>
      </div>

      {isDockrevService(service) && supervisorState.status === 'offline' ? (
        <div className="muted" style={{ marginTop: 10 }}>
          supervisor offline · {supervisorErrorAt ?? '-'}
        </div>
      ) : null}

      <div className="twoCol">
        <div className="card">
          <div className="title">更新策略</div>

          <div className="ruleList">
            {rules.map((r) => (
              <div key={r.id} className="ruleRow" style={{ display: 'flex', gap: 12, alignItems: 'flex-start' }}>
                <div style={{ flex: 1 }}>
                  <div className="mono">
                    {r.match.kind}={r.match.value}
                  </div>
                  <div className="muted">
                    id <Mono>{r.id}</Mono> · enabled <Mono>{String(r.enabled)}</Mono>
                    {r.note ? (
                      <>
                        {' '}
                        · note <Mono>{r.note}</Mono>
                      </>
                    ) : null}
                  </div>
                </div>
                <Button
                  variant="ghost"
                  disabled={busy}
                  onClick={() => {
                    void (async () => {
                      setBusy(true)
                      setError(null)
                      try {
                        await deleteIgnore(r.id)
                        await refresh()
                      } catch (e: unknown) {
                        setError(errorMessage(e))
                      } finally {
                        setBusy(false)
                      }
                    })()
                  }}
                >
                  删除
                </Button>
              </div>
            ))}
            {rules.length === 0 ? <div className="muted">暂无规则</div> : null}
          </div>

          <div className="sectionTitle" style={{ marginTop: 14 }}>
            添加规则
          </div>
          <div className="formGrid">
            <label className="formField">
              <span className="label">Kind</span>
              <select
                className="input"
                value={newRuleKind}
                onChange={(e) => setNewRuleKind(e.target.value as 'exact' | 'prefix' | 'regex' | 'semver')}
              >
                <option value="exact">exact</option>
                <option value="prefix">prefix</option>
                <option value="regex">regex</option>
                <option value="semver">semver</option>
              </select>
            </label>
            <label className="formField formSpan2">
              <span className="label">Value</span>
              <input className="input" value={newRuleValue} onChange={(e) => setNewRuleValue(e.target.value)} />
            </label>
            <label className="formField formSpan2">
              <span className="label">Note</span>
              <input className="input" value={newRuleNote} onChange={(e) => setNewRuleNote(e.target.value)} />
            </label>
            <div className="formActions formSpan2">
              <Button
                variant="primary"
                disabled={busy}
                onClick={() => {
                  void (async () => {
                    setBusy(true)
                    setError(null)
                    try {
                      await createIgnore({
                        enabled: true,
                        serviceId,
                        kind: newRuleKind,
                        value: newRuleValue,
                        note: newRuleNote,
                      })
                      await refresh()
                    } catch (e: unknown) {
                      setError(errorMessage(e))
                    } finally {
                      setBusy(false)
                    }
                  })()
                }}
              >
                添加
              </Button>
            </div>
          </div>
        </div>

        <div className="card">
          <div className="title">更新前备份 / 回滚</div>
          <div className="muted">服务级策略（失败回滚 + 备份 targets 三态选择）</div>

          <div className="kv">
            <div className="kvRow">
              <div className="label">失败回滚（autoRollback）</div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <Switch checked={settings.autoRollback} disabled={busy} onChange={(v) => setSettings({ ...settings, autoRollback: v })} />
                <div className="muted">{settings.autoRollback ? 'on' : 'off'}</div>
              </div>
            </div>
          </div>

          <div className="sectionTitle" style={{ marginTop: 14 }}>
            备份项（服务级）
          </div>
          <div className="muted">三态：inherit / skip / force</div>

          <div className="kv" style={{ marginTop: 10 }}>
            <div className="label">Bind paths</div>
            {bindTargets.length === 0 ? <div className="muted">暂无</div> : null}
            {bindTargets.map((t) => (
              <div key={t.key} className="kvRow">
                <div className="mono">{t.key}</div>
                <select
                  className="input"
                  value={t.value}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      backupTargets: {
                        ...settings.backupTargets,
                        bindPaths: {
                          ...settings.backupTargets.bindPaths,
                          [t.key]: e.target.value as 'inherit' | 'skip' | 'force',
                        },
                      },
                    })
                  }
                >
                  <option value="inherit">inherit</option>
                  <option value="skip">skip</option>
                  <option value="force">force</option>
                </select>
              </div>
            ))}

            <div className="label" style={{ marginTop: 10 }}>
              Volume names
            </div>
            {volTargets.length === 0 ? <div className="muted">暂无</div> : null}
            {volTargets.map((t) => (
              <div key={t.key} className="kvRow">
                <div className="mono">{t.key}</div>
                <select
                  className="input"
                  value={t.value}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      backupTargets: {
                        ...settings.backupTargets,
                        volumeNames: {
                          ...settings.backupTargets.volumeNames,
                          [t.key]: e.target.value as 'inherit' | 'skip' | 'force',
                        },
                      },
                    })
                  }
                >
                  <option value="inherit">inherit</option>
                  <option value="skip">skip</option>
                  <option value="force">force</option>
                </select>
              </div>
            ))}

            <div className="formActions">
              <Button
                variant="primary"
                disabled={busy}
                onClick={() => {
                  void (async () => {
                    setBusy(true)
                    setError(null)
                    try {
                      await putServiceSettings(props.serviceId, settings)
                      await refresh()
                    } catch (e: unknown) {
                      setError(errorMessage(e))
                    } finally {
                      setBusy(false)
                    }
                  })()
                }}
              >
                保存服务设置
              </Button>
            </div>
          </div>
        </div>
      </div>

      <div className="card" style={{ marginTop: 16 }}>
        <div className="title">Webhook 触发（服务级）</div>
        <div className="muted">用于外部系统触发：更新此服务 / 更新 compose / 更新全部</div>

        <div className="webhookRow">
          <div className="label">POST</div>
          <div className="mono">/api/v1/update/service/{service.name}</div>
          <div style={{ marginLeft: 'auto' }} className="chipStatic">
            需要鉴权
          </div>
        </div>
        <div className="webhookBody">
          <div className="label">Body（可选）</div>
          <div className="mono">{`{ "dryRun": true, "backup": "inherit" }`}</div>
          <div className="muted">dryRun=仅预览；backup=inherit/on/off；rollback=inherit/on/off</div>
        </div>
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
    </div>
  )
}
