import type {
  CleanupApplyRequest,
  CleanupFingerprintMismatchError,
  CleanupPreset,
  CleanupResourceItem,
  CleanupResourceKind,
  CleanupScanRequest,
  CleanupScanResponse,
  CleanupStackGroup,
} from '../../api'

export type CleanupMockScenario =
  | 'cleanup-console'
  | 'cleanup-console-storage-normal'
  | 'cleanup-console-empty'
  | 'cleanup-console-aggressive-unowned'
  | 'cleanup-console-stale'
  | 'cleanup-console-confirm-pending'
  | 'cleanup-console-confirm-failed'
  | 'cleanup-console-scan-pending'
  | 'cleanup-console-scan-slow'
  | 'cleanup-console-apply-slow'
  | 'cleanup-console-unknown-volume-only'

export type CleanupMockRuntimeState = {
  nextJobSeq: number
  staleApplyConsumed: boolean
  confirmPendingConsumed: boolean
  confirmFailureConsumed: boolean
  nextScanRunSeq: number
  scanRuns: Map<string, Array<{ id: number; event: string; data: unknown }>>
}

type CleanupOwner =
  | {
      kind: 'service'
      stackId: string
      stackName: string
      serviceId: string
      serviceName: string
    }
  | {
      kind: 'stack'
      stackId: string
      stackName: string
    }
  | {
      kind: 'unowned'
      title: string
    }

type CleanupEntry = {
  resourceId: string
  kind: CleanupResourceKind
  label: string
  reason: string
  minPreset: CleanupPreset
  estimatedReclaimableBytes?: number | null
  estimateUnknown?: boolean
  owner: CleanupOwner
}

const PRESET_ORDER: CleanupPreset[] = ['conservative', 'balanced', 'project_deep_clean', 'aggressive']
const UNOWNED_TITLE = '未归属资源'

export function isCleanupMockScenario(value: string): value is CleanupMockScenario {
  return (
    value === 'cleanup-console' ||
    value === 'cleanup-console-storage-normal' ||
    value === 'cleanup-console-empty' ||
    value === 'cleanup-console-aggressive-unowned' ||
    value === 'cleanup-console-stale' ||
    value === 'cleanup-console-confirm-pending' ||
    value === 'cleanup-console-confirm-failed' ||
    value === 'cleanup-console-scan-pending' ||
    value === 'cleanup-console-scan-slow' ||
    value === 'cleanup-console-apply-slow' ||
    value === 'cleanup-console-unknown-volume-only'
  )
}

function presetIncludes(active: CleanupPreset, minPreset: CleanupPreset): boolean {
  return PRESET_ORDER.indexOf(active) >= PRESET_ORDER.indexOf(minPreset)
}

function toResource(entry: CleanupEntry): CleanupResourceItem {
  return {
    resourceId: entry.resourceId,
    kind: entry.kind,
    label: entry.label,
    reason: entry.reason,
    minPreset: entry.minPreset,
    estimatedReclaimableBytes: entry.estimatedReclaimableBytes ?? null,
    estimateUnknown: entry.estimateUnknown === true || entry.estimatedReclaimableBytes == null,
  }
}

function itemHasUnknown(item: CleanupResourceItem): boolean {
  return item.estimateUnknown === true || item.estimatedReclaimableBytes == null
}

function sumKnown(items: CleanupResourceItem[]): number {
  return items.reduce((sum, item) => sum + (item.estimatedReclaimableBytes ?? 0), 0)
}

function entriesForScenario(
  scenario: CleanupMockScenario,
  revision: 1 | 2 = 1,
): { entries: CleanupEntry[]; scannedAt: string } {
  if (scenario === 'cleanup-console-empty') {
    return { entries: [], scannedAt: '2026-03-29T10:00:00Z' }
  }

  if (scenario === 'cleanup-console-unknown-volume-only') {
    return {
      entries: [
        {
          resourceId: 'prod-api-volume-unknown',
          kind: 'volume',
          label: 'prod_api_cache_unknown',
          reason: '卷未挂载到任何容器',
          minPreset: 'project_deep_clean',
          estimatedReclaimableBytes: null,
          estimateUnknown: true,
          owner: {
            kind: 'service',
            stackId: 'stack-prod',
            stackName: 'prod',
            serviceId: 'svc-prod-api',
            serviceName: 'api',
          },
        },
      ],
      scannedAt: '2026-03-29T10:05:00Z',
    }
  }

  const base: CleanupEntry[] = [
    {
      resourceId: 'prod-network',
      kind: 'network',
      label: 'prod_default',
      reason: '网络没有活动容器连接',
      minPreset: 'conservative',
      estimatedReclaimableBytes: 12 * 1024 * 1024,
      owner: { kind: 'stack', stackId: 'stack-prod', stackName: 'prod' },
    },
    {
      resourceId: 'prod-api-container',
      kind: 'container',
      label: 'prod-api-exited-20260328',
      reason: '容器已退出',
      minPreset: 'conservative',
      estimatedReclaimableBytes: 480 * 1024 * 1024,
      owner: {
        kind: 'service',
        stackId: 'stack-prod',
        stackName: 'prod',
        serviceId: 'svc-prod-api',
        serviceName: 'api',
      },
    },
    {
      resourceId: 'prod-api-image',
      kind: 'image',
      label: revision === 2 ? 'ghcr.io/acme/api@sha256:cleanup-newer' : 'ghcr.io/acme/api@sha256:cleanup-old',
      reason: '旧镜像未被任何容器使用',
      minPreset: 'balanced',
      estimatedReclaimableBytes: (revision === 2 ? 1710 : 1430) * 1024 * 1024,
      owner: {
        kind: 'service',
        stackId: 'stack-prod',
        stackName: 'prod',
        serviceId: 'svc-prod-api',
        serviceName: 'api',
      },
    },
    {
      resourceId: 'prod-api-volume',
      kind: 'volume',
      label: 'prod_api_cache',
      reason: '卷未挂载到任何容器',
      minPreset: 'project_deep_clean',
      estimatedReclaimableBytes: null,
      estimateUnknown: true,
      owner: {
        kind: 'service',
        stackId: 'stack-prod',
        stackName: 'prod',
        serviceId: 'svc-prod-api',
        serviceName: 'api',
      },
    },
    {
      resourceId: 'prod-worker-container',
      kind: 'container',
      label: 'prod-worker-exited-20260327',
      reason: '容器已退出',
      minPreset: 'conservative',
      estimatedReclaimableBytes: 110 * 1024 * 1024,
      owner: {
        kind: 'service',
        stackId: 'stack-prod',
        stackName: 'prod',
        serviceId: 'svc-prod-worker',
        serviceName: 'worker',
      },
    },
    {
      resourceId: 'infra-network',
      kind: 'network',
      label: 'infra_metrics',
      reason: '网络没有活动容器连接',
      minPreset: 'conservative',
      estimatedReclaimableBytes: 4 * 1024 * 1024,
      owner: { kind: 'stack', stackId: 'stack-infra', stackName: 'infra' },
    },
    {
      resourceId: 'infra-postgres-volume',
      kind: 'volume',
      label: 'infra_pgdata_tmp',
      reason: '卷未挂载到任何容器',
      minPreset: 'project_deep_clean',
      estimatedReclaimableBytes: 2200 * 1024 * 1024,
      owner: {
        kind: 'service',
        stackId: 'stack-infra',
        stackName: 'infra',
        serviceId: 'svc-infra-postgres',
        serviceName: 'postgres',
      },
    },
    {
      resourceId: 'infra-prometheus-image',
      kind: 'image',
      label: 'quay.io/prometheus/prometheus@sha256:unused',
      reason: '旧镜像未被任何容器使用',
      minPreset: 'balanced',
      estimatedReclaimableBytes: 620 * 1024 * 1024,
      owner: {
        kind: 'service',
        stackId: 'stack-infra',
        stackName: 'infra',
        serviceId: 'svc-infra-prometheus',
        serviceName: 'prometheus',
      },
    },
  ]

  if (scenario === 'cleanup-console-aggressive-unowned') {
    base.push(
      {
        resourceId: 'builder-cache',
        kind: 'builder_cache',
        label: 'buildx local cache',
        reason: 'Builder cache 可回收',
        minPreset: 'balanced',
        estimatedReclaimableBytes: 640 * 1024 * 1024,
        owner: { kind: 'unowned', title: UNOWNED_TITLE },
      },
      {
        resourceId: 'global-unused-image',
        kind: 'image',
        label: 'sha256:global-unused-image',
        reason: '未归属镜像未被任何容器使用',
        minPreset: 'aggressive',
        estimatedReclaimableBytes: 1830 * 1024 * 1024,
        owner: { kind: 'unowned', title: UNOWNED_TITLE },
      },
      {
        resourceId: 'global-unused-volume',
        kind: 'volume',
        label: 'docker_orphan_volume',
        reason: '未归属卷未挂载到任何容器',
        minPreset: 'aggressive',
        estimatedReclaimableBytes: null,
        estimateUnknown: true,
        owner: { kind: 'unowned', title: UNOWNED_TITLE },
      },
    )
  }

  if (scenario === 'cleanup-console-stale' && revision === 2) {
    base.push({
      resourceId: 'prod-worker-image',
      kind: 'image',
      label: 'ghcr.io/acme/worker@sha256:late-candidate',
      reason: '旧镜像未被任何容器使用',
      minPreset: 'balanced',
      estimatedReclaimableBytes: 540 * 1024 * 1024,
      owner: {
        kind: 'service',
        stackId: 'stack-prod',
        stackName: 'prod',
        serviceId: 'svc-prod-worker',
        serviceName: 'worker',
      },
    })
  }

  if (scenario === 'cleanup-console-storage-normal') {
    return {
      entries: base.map((entry) =>
        entry.resourceId === 'prod-api-volume'
          ? {
              ...entry,
              estimatedReclaimableBytes: 860 * 1024 * 1024,
              estimateUnknown: false,
            }
          : entry,
      ),
      scannedAt: '2026-03-29T10:05:00Z',
    }
  }

  return {
    entries: base,
    scannedAt: revision === 2 ? '2026-03-29T10:08:00Z' : '2026-03-29T10:05:00Z',
  }
}

function selectEntries(entries: CleanupEntry[], request: CleanupScanRequest): CleanupEntry[] {
  const byPreset = entries.filter((entry) => presetIncludes(request.preset, entry.minPreset))
  if (request.scope === 'all') return byPreset
  if (request.scope === 'stack') {
    return byPreset.filter((entry) => {
      if (entry.owner.kind === 'unowned') return false
      return entry.owner.stackId === request.stackId
    })
  }
  return byPreset.filter(
    (entry) =>
      entry.owner.kind === 'service' &&
      entry.owner.stackId === request.stackId &&
      entry.owner.serviceId === request.serviceId,
  )
}

function buildFingerprint(
  scenario: CleanupMockScenario,
  request: CleanupScanRequest,
  revision: 1 | 2,
  entries: CleanupEntry[],
): string {
  return [
    'cleanup',
    scenario,
    request.scope,
    request.preset,
    request.stackId ?? '-',
    request.serviceId ?? '-',
    `r${revision}`,
    entries.map((entry) => entry.resourceId).join(','),
  ].join(':')
}

export function buildCleanupMockScanResponse(
  scenario: CleanupMockScenario,
  request: CleanupScanRequest,
  revision: 1 | 2 = 1,
): CleanupScanResponse {
  const fixture = entriesForScenario(scenario, revision)
  const selected = selectEntries(fixture.entries, request)
  const stackMap = new Map<
    string,
    {
      stackId: string
      stackName: string
      stackOrphans: CleanupResourceItem[]
      services: Map<string, { serviceId: string; serviceName: string; resources: CleanupResourceItem[] }>
    }
  >()
  const unowned: CleanupResourceItem[] = []

  for (const entry of selected) {
    const item = toResource(entry)
    if (entry.owner.kind === 'unowned') {
      unowned.push(item)
      continue
    }

    const existingStack = stackMap.get(entry.owner.stackId) ?? {
      stackId: entry.owner.stackId,
      stackName: entry.owner.stackName,
      stackOrphans: [] as CleanupResourceItem[],
      services: new Map<string, { serviceId: string; serviceName: string; resources: CleanupResourceItem[] }>(),
    }
    stackMap.set(entry.owner.stackId, existingStack)

    if (entry.owner.kind === 'stack') {
      existingStack.stackOrphans.push(item)
      continue
    }

    const service =
      existingStack.services.get(entry.owner.serviceId) ?? {
        serviceId: entry.owner.serviceId,
        serviceName: entry.owner.serviceName,
        resources: [] as CleanupResourceItem[],
      }
    service.resources.push(item)
    existingStack.services.set(entry.owner.serviceId, service)
  }

  const stackGroups: CleanupStackGroup[] = [...stackMap.values()]
    .map((stack) => {
      const services = [...stack.services.values()]
        .map((service) => ({
          serviceId: service.serviceId,
          serviceName: service.serviceName,
          resources: service.resources,
          estimatedReclaimableBytes: sumKnown(service.resources),
          hasUnknownSize: service.resources.some(itemHasUnknown),
        }))
        .sort((left, right) => left.serviceName.localeCompare(right.serviceName))
      const groupResources = [...stack.stackOrphans, ...services.flatMap((service) => service.resources)]
      return {
        stackId: stack.stackId,
        stackName: stack.stackName,
        stackOrphans: stack.stackOrphans,
        services,
        estimatedReclaimableBytes: sumKnown(groupResources),
        hasUnknownSize: groupResources.some(itemHasUnknown),
      }
    })
    .sort((left, right) => left.stackName.localeCompare(right.stackName))

  const unownedGroup =
    request.scope === 'all' && unowned.length > 0
      ? {
          title: UNOWNED_TITLE,
          resources: unowned,
          estimatedReclaimableBytes: sumKnown(unowned),
          hasUnknownSize: unowned.some(itemHasUnknown),
        }
      : null

  const allResources = [...stackGroups.flatMap((stack) => [...stack.stackOrphans, ...stack.services.flatMap((service) => service.resources)]), ...unowned]
  return {
    status: 'ready',
    reason: request.reason,
    preset: request.preset,
    scope: request.scope,
    scannedAt: fixture.scannedAt,
    refreshing: false,
    retryAfterMs: null,
    estimatedReclaimableBytes: sumKnown(allResources),
    hasUnknownSize: allResources.some(itemHasUnknown),
    serverDiskUsage: {
      usedBytes: 40_587_440_947,
      totalBytes: 85_899_345_920,
    },
    stackGroups,
    unownedGroup,
    confirmationFingerprint:
      request.reason === 'confirm' ? buildFingerprint(scenario, request, revision, selected) : buildFingerprint(scenario, request, revision, selected),
  }
}

export function resolveCleanupMockApply(
  scenario: CleanupMockScenario,
  request: CleanupApplyRequest,
  runtime: CleanupMockRuntimeState,
): { ok: true; jobId: string } | { ok: false; status: number; body: { error: { code: string; message: string; details: CleanupFingerprintMismatchError } } } {
  if (scenario === 'cleanup-console-stale' && !runtime.staleApplyConsumed) {
    runtime.staleApplyConsumed = true
    const latest = buildCleanupMockScanResponse(
      scenario,
      {
        reason: 'confirm',
        preset: request.preset,
        scope: request.scope,
        stackId: request.stackId,
        serviceId: request.serviceId,
      },
      2,
    )
    return {
      ok: false,
      status: 409,
      body: {
        error: {
          code: 'cleanup_snapshot_stale',
          message: 'cleanup snapshot changed; please confirm again',
          details: { latest },
        },
      },
    }
  }

  runtime.nextJobSeq += 1
  return {
    ok: true,
    jobId: `job-cleanup-${runtime.nextJobSeq}`,
  }
}
