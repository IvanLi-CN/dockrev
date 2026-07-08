import { useCallback, useEffect, useState, type ReactNode } from 'react'
import {
  getStack,
  getStackSettings,
  listJobs,
  putStackSettings,
  type JobListItem,
  type StackDetail,
  type StackSettings,
} from '../api'
import { createDefaultAutoUpdatePolicy } from '../components/AutoUpdatePolicyEditor'
import { AutoUpdatePolicyDrawer } from '../components/AutoUpdatePolicyDrawer'
import { AutoUpdatePolicyResultCard } from '../components/AutoUpdatePolicyResultCard'
import { RecentUpdateRecords, selectRecentStackUpdateJobs } from '../components/RecentUpdateRecords'
import { ReadonlySnapshotNotice } from '../components/ReadonlySnapshotNotice'
import { usePwaStatus } from '../pwaStatus'
import { buildReadonlySnapshotKey, readReadonlySnapshot, writeReadonlySnapshot } from '../readonlySnapshotCache'
import { navigate } from '../routes'
import { Button, Mono, Pill } from '../ui'
import { serviceRowStatus } from '../updateStatus'

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function statusLabel(status: ReturnType<typeof serviceRowStatus>): string {
  if (status === 'updatable') return '可更新'
  if (status === 'archMismatch') return '架构不匹配'
  if (status === 'blocked') return '被阻止'
  if (status === 'hint') return '需确认'
  return '无候选'
}

function statusTone(status: ReturnType<typeof serviceRowStatus>): 'ok' | 'warn' | 'bad' | 'muted' | 'info' {
  if (status === 'updatable') return 'ok'
  if (status === 'archMismatch' || status === 'hint') return 'warn'
  if (status === 'blocked') return 'bad'
  return 'muted'
}

const STACK_DETAIL_SNAPSHOT_STALE_MS = 60_000

type StackDetailSnapshotPayload = {
  stack: StackDetail
  jobs: JobListItem[]
  policy: StackSettings['autoUpdatePolicy'] | null
}

export function StackDetailPage(props: {
  stackId: string
  onLastScanHint: (lastScan?: string) => void
  onTopActions: (node: ReactNode) => void
}) {
  const { stackId, onLastScanHint, onTopActions } = props
  const { isOnline } = usePwaStatus()
  const snapshotKey = buildReadonlySnapshotKey('stack-detail', stackId)
  const [stack, setStack] = useState<StackDetail | null>(null)
  const [settings, setSettings] = useState<StackSettings | null>(null)
  const [cachedPolicy, setCachedPolicy] = useState<StackSettings['autoUpdatePolicy'] | null>(null)
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [settingsDrawerOpen, setSettingsDrawerOpen] = useState(false)
  const [autoPolicyDraft, setAutoPolicyDraft] = useState(() => createDefaultAutoUpdatePolicy('override'))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [, setSnapshotStatus] = useState<'missing' | 'fresh' | 'stale' | 'expired' | 'unsupported'>(
    'missing',
  )
  const [snapshotFetchedAt, setSnapshotFetchedAt] = useState<string | null>(null)
  const [snapshotAnchorFetchedAt, setSnapshotAnchorFetchedAt] = useState<string | null>(null)
  const [snapshotActive, setSnapshotActive] = useState(false)

  const refresh = useCallback(async () => {
    setError(null)
    onLastScanHint(undefined)
    const [stackRes, settingsRes, jobsRes] = await Promise.all([
      getStack(stackId),
      getStackSettings(stackId),
      listJobs().catch(() => []),
    ])
    setStack(stackRes)
    setSettings(settingsRes)
    setCachedPolicy(settingsRes.autoUpdatePolicy ?? null)
    setJobs(jobsRes)
    setSnapshotActive(false)
    setSnapshotAnchorFetchedAt(null)
  }, [onLastScanHint, stackId])

  useEffect(() => {
    let cancelled = false
    void (async () => {
      const snapshot = await readReadonlySnapshot<StackDetailSnapshotPayload>(snapshotKey)
      if (cancelled) return
      setSnapshotStatus(snapshot.status)
      setSnapshotFetchedAt(snapshot.record?.fetchedAt ?? null)
      setSnapshotAnchorFetchedAt(snapshot.record?.fetchedAt ?? null)
      if (snapshot.status !== 'fresh') return
      setStack(snapshot.record.payload.stack)
      setJobs(snapshot.record.payload.jobs)
      setCachedPolicy(snapshot.record.payload.policy ?? null)
      setSnapshotActive(true)
    })()
    return () => {
      cancelled = true
    }
  }, [snapshotKey])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(errorMessage(e)))
  }, [refresh])

  useEffect(() => {
    if (!stack) return
    void writeReadonlySnapshot(
      snapshotKey,
      {
        stack,
        jobs,
        policy: settings?.autoUpdatePolicy ?? cachedPolicy ?? null,
      },
      {
        staleAfterMs: STACK_DETAIL_SNAPSHOT_STALE_MS,
        fetchedAt: snapshotAnchorFetchedAt ? Date.parse(snapshotAnchorFetchedAt) || undefined : undefined,
      },
    )
  }, [cachedPolicy, jobs, settings?.autoUpdatePolicy, snapshotAnchorFetchedAt, snapshotKey, stack])

  useEffect(() => {
    onTopActions(
      <>
        <Button disabled={busy} onClick={() => navigate({ name: 'services' })}>
          返回服务
        </Button>
        <Button disabled={busy || !isOnline} onClick={() => void refresh()}>
          刷新
        </Button>
      </>,
    )
    return () => onTopActions(null)
  }, [busy, isOnline, onTopActions, refresh])

  if (!stack) {
    if (!isOnline) {
      return (
        <div className="page">
          <ReadonlySnapshotNotice
            tone="bad"
            title="当前没有可用的离线 Stack 数据。"
            detail="请恢复联网后重新加载该页面。"
          />
        </div>
      )
    }
    return <div className="muted">加载中…</div>
  }

  const policy = settings?.autoUpdatePolicy ?? cachedPolicy ?? createDefaultAutoUpdatePolicy('override')
  const updatable = stack.services.filter((service) => serviceRowStatus(service) !== 'ok').length
  const recentUpdateJobs = selectRecentStackUpdateJobs(jobs, stack)

  return (
    <div className="page">
      {snapshotActive ? (
        <ReadonlySnapshotNotice
          tone={!isOnline ? 'warn' : 'info'}
          title={!isOnline ? '当前离线，显示已缓存的 Stack 数据。' : '先显示已缓存的 Stack 数据，后台会继续刷新。'}
          detail="自动更新策略只展示只读摘要；恢复联网后才能编辑并保存。"
          fetchedAt={snapshotFetchedAt}
          actionLabel="重试刷新"
          actionDisabled={!isOnline || busy}
          onAction={() => void refresh()}
        />
      ) : null}
      <div className="stackDetailHero">
        <div>
          <div className="svcTitleName">Stack: <Mono>{stack.name}</Mono></div>
          <div className="muted">id <Mono>{stack.id}</Mono> · 服务 {stack.services.length} · 候选 {updatable}</div>
        </div>
        <Pill tone={stack.archived ? 'warn' : 'info'}>{stack.archived ? 'archived' : stack.compose.type}</Pill>
      </div>

      <div className="settingsSummaryGrid">
      <AutoUpdatePolicyResultCard
        busy={busy}
        onOpenSettings={() => {
          if (!settings || !isOnline) return
          setAutoPolicyDraft(policy)
          setSettingsDrawerOpen(true)
        }}
        policy={policy}
        scope="stack"
      />
        <RecentUpdateRecords jobs={recentUpdateJobs} />
      </div>

      <div className="card" style={{ marginTop: 16 }}>
        <div className="title">服务</div>
        <div className="stackServiceList">
          {stack.services.map((service) => {
            const status = serviceRowStatus(service)
            return (
              <div className="stackServiceRow" key={service.id}>
                <div>
                  <div className="mono">{service.name}</div>
                  <div className="muted">{service.image.ref}</div>
                </div>
                <Pill tone={statusTone(status)}>{statusLabel(status)}</Pill>
                <Button onClick={() => navigate({ name: 'service', stackId: stack.id, serviceId: service.id })}>
                  详情
                </Button>
              </div>
            )
          })}
        </div>
      </div>
      <AutoUpdatePolicyDrawer
        busy={busy}
        onChange={setAutoPolicyDraft}
        onOpenChange={setSettingsDrawerOpen}
        onSave={() => {
          void (async () => {
            setBusy(true)
            setError(null)
            try {
              await putStackSettings(stack.id, { autoUpdatePolicy: autoPolicyDraft })
              await refresh()
            } catch (e: unknown) {
              setError(errorMessage(e))
            } finally {
              setBusy(false)
            }
          })()
        }}
        open={settingsDrawerOpen}
        policy={autoPolicyDraft}
        scope="stack"
      />
      {error ? <div className="error">{error}</div> : null}
    </div>
  )
}
