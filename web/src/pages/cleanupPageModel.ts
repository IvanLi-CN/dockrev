import { ApiError, type CleanupPreset, type CleanupResourceItem, type CleanupResourceKind, type CleanupScanResponse, type CleanupServerDiskUsage, type CleanupStackGroup } from '../api'

const PRESET_ORDER: CleanupPreset[] = ['conservative', 'balanced', 'project_deep_clean', 'aggressive']

export const KIND_LABEL: Record<CleanupResourceKind, string> = {
  image: '镜像',
  container: '容器',
  network: '网络',
  volume: '卷',
  builder_cache: 'Builder Cache',
}

export type CleanupUsageBucket = 'container' | 'image' | 'volume' | 'other'

export type CleanupUsageCard = {
  key: CleanupUsageBucket
  label: string
  description: string
  bytes: number
  count: number
  unknownCount: number
  share: number
}

const CLEANUP_USAGE_CARD_COPY: Record<CleanupUsageBucket, { label: string; description: string }> = {
  container: {
    label: '容器',
    description: '可回收容器层与残留运行时空间',
  },
  image: {
    label: '镜像',
    description: '可回收旧 tag 与未被引用的镜像层',
  },
  volume: {
    label: '卷',
    description: '可回收缓存、数据卷与未挂载持久化内容',
  },
  other: {
    label: '其他',
    description: '可回收网络与 builder cache 等辅助资源',
  },
}

const USAGE_BUCKETS: CleanupUsageBucket[] = ['container', 'image', 'volume', 'other']

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = bytes
  let index = 0
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024
    index += 1
  }
  const digits = value >= 100 || index === 0 ? 0 : value >= 10 ? 1 : 2
  return `${value.toFixed(digits)} ${units[index]}`
}

export function formatEstimate(bytes?: number | null, hasUnknown?: boolean): string {
  const normalizedBytes = Number.isFinite(bytes ?? null) ? Math.max(0, bytes ?? 0) : 0
  if (hasUnknown) {
    if (normalizedBytes > 0) return `${formatBytes(normalizedBytes)}+`
    return '未知大小'
  }
  return formatBytes(normalizedBytes)
}

export function formatDiskUsage(usage?: CleanupServerDiskUsage | null): { value: string; hint: string; percent: number } {
  if (!usage || usage.totalBytes <= 0) {
    return {
      value: '未获取',
      hint: '运行环境未返回 df 数据',
      percent: 0,
    }
  }
  const percent = Math.min(1, Math.max(0, usage.usedBytes / usage.totalBytes))
  return {
    value: `${formatBytes(usage.usedBytes)} / ${formatBytes(usage.totalBytes)}`,
    hint: `服务器磁盘已使用 ${formatPercent(percent)}`,
    percent,
  }
}

export function formatPercent(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0%'
  if (value >= 0.995) return '100%'
  return `${Math.max(1, Math.round(value * 100))}%`
}

export function formatUnknownCount(count: number): string {
  return `${count} 项大小未知`
}

export function countVisibleResources(response: CleanupScanResponse): number {
  if (response.status !== 'ready') return 0
  let total = 0
  for (const stack of response.stackGroups) {
    total += stack.stackOrphans.length
    for (const service of stack.services) total += service.resources.length
  }
  total += response.unownedGroup?.resources.length ?? 0
  return total
}

export function countRenderableResources(response: CleanupScanResponse): number {
  let total = 0
  for (const stack of response.stackGroups) {
    total += stack.stackOrphans.length
    for (const service of stack.services) total += service.resources.length
  }
  total += response.unownedGroup?.resources.length ?? 0
  return total
}

export function flattenAllResources(response: CleanupScanResponse): CleanupResourceItem[] {
  return [
    ...response.stackGroups.flatMap((stack) => [
      ...stack.stackOrphans,
      ...stack.services.flatMap((service) => service.resources),
    ]),
    ...(response.unownedGroup?.resources ?? []),
  ]
}

export function aggregateStackResources(stack: CleanupStackGroup): CleanupResourceItem[] {
  return [...stack.stackOrphans, ...stack.services.flatMap((service) => service.resources)]
}

export function countUnknownResources(resources: CleanupResourceItem[]): number {
  return resources.filter(itemHasUnknownSize).length
}

function usageBucketForKind(kind: CleanupResourceKind): CleanupUsageBucket {
  if (kind === 'container') return 'container'
  if (kind === 'image') return 'image'
  if (kind === 'volume') return 'volume'
  return 'other'
}

export function buildUsageCards(response: CleanupScanResponse): CleanupUsageCard[] {
  const resources = flattenAllResources(response)
  const totals = new Map<CleanupUsageBucket, { bytes: number; count: number; unknownCount: number }>()
  for (const key of USAGE_BUCKETS) totals.set(key, { bytes: 0, count: 0, unknownCount: 0 })

  for (const resource of resources) {
    const bucket = usageBucketForKind(resource.kind)
    const entry = totals.get(bucket)
    if (!entry) continue
    entry.count += 1
    entry.bytes += resource.estimatedReclaimableBytes ?? 0
    if (itemHasUnknownSize(resource)) entry.unknownCount += 1
  }

  const totalKnownBytes = resources.reduce((sum, resource) => sum + (resource.estimatedReclaimableBytes ?? 0), 0)
  return USAGE_BUCKETS.map((key) => {
    const entry = totals.get(key) ?? { bytes: 0, count: 0, unknownCount: 0 }
    return {
      key,
      label: CLEANUP_USAGE_CARD_COPY[key].label,
      description: CLEANUP_USAGE_CARD_COPY[key].description,
      bytes: entry.bytes,
      count: entry.count,
      unknownCount: entry.unknownCount,
      share: totalKnownBytes > 0 ? entry.bytes / totalKnownBytes : 0,
    }
  })
}

export function kindSummary(resources: CleanupResourceItem[]): string {
  const counts = new Map<CleanupResourceKind, number>()
  for (const resource of resources) {
    counts.set(resource.kind, (counts.get(resource.kind) ?? 0) + 1)
  }
  return [...counts.entries()]
    .map(([kind, count]) => `${KIND_LABEL[kind]} ${count}`)
    .join(' · ')
}

export function toErrorMessage(error: unknown): string {
  if (error instanceof ApiError) return error.message
  if (error instanceof Error && error.message.trim()) return error.message
  return '请求失败，请稍后重试。'
}

function includesPreset(active: CleanupPreset, candidate: CleanupPreset): boolean {
  return PRESET_ORDER.indexOf(active) >= PRESET_ORDER.indexOf(candidate)
}

export function itemHasUnknownSize(item: CleanupResourceItem): boolean {
  return item.estimateUnknown === true || item.estimatedReclaimableBytes == null
}

export function cleanupResourceKey(item: CleanupResourceItem): string {
  return `${item.kind}:${item.resourceId}`
}

export function cleanupResourceKeys(response: CleanupScanResponse): Set<string> {
  return new Set(flattenAllResources(response).map(cleanupResourceKey))
}

function cleanupUsageBucketKey(item: CleanupResourceItem): CleanupUsageBucket {
  return usageBucketForKind(item.kind)
}

export function staleBucketsForResponse(response: CleanupScanResponse | null, staleKeys: Set<string>): Set<CleanupUsageBucket> {
  const buckets = new Set<CleanupUsageBucket>()
  if (!response || staleKeys.size === 0) return buckets
  for (const resource of flattenAllResources(response)) {
    if (staleKeys.has(cleanupResourceKey(resource))) buckets.add(cleanupUsageBucketKey(resource))
  }
  return buckets
}

function mergeResourceLists(
  previous: CleanupResourceItem[],
  partial: CleanupResourceItem[],
): { resources: CleanupResourceItem[]; staleKeys: Set<string> } {
  const partialKeys = new Set(partial.map(cleanupResourceKey))
  const staleKeys = new Set<string>()
  const merged = [...partial]
  for (const item of previous) {
    const key = cleanupResourceKey(item)
    if (partialKeys.has(key)) continue
    staleKeys.add(key)
    merged.push(item)
  }
  return {
    resources: merged,
    staleKeys,
  }
}

function sumKnownResources(resources: CleanupResourceItem[]): number {
  return resources.reduce((sum, item) => sum + (item.estimatedReclaimableBytes ?? 0), 0)
}

export function mergeCleanupResponses(
  previous: CleanupScanResponse,
  partial: CleanupScanResponse,
): { response: CleanupScanResponse; staleKeys: Set<string> } {
  const staleKeys = new Set<string>()
  const previousStacks = new Map(previous.stackGroups.map((stack) => [stack.stackId, stack]))
  const partialStacks = new Map(partial.stackGroups.map((stack) => [stack.stackId, stack]))
  const stackIds = new Set([...previousStacks.keys(), ...partialStacks.keys()])
  const stackGroups: CleanupStackGroup[] = []

  for (const stackId of stackIds) {
    const oldStack = previousStacks.get(stackId)
    const newStack = partialStacks.get(stackId)
    if (!oldStack && newStack) {
      stackGroups.push(newStack)
      continue
    }
    if (oldStack && !newStack) {
      for (const resource of aggregateStackResources(oldStack)) staleKeys.add(cleanupResourceKey(resource))
      stackGroups.push(oldStack)
      continue
    }
    if (!oldStack || !newStack) continue

    const mergedOrphans = mergeResourceLists(oldStack.stackOrphans, newStack.stackOrphans)
    for (const key of mergedOrphans.staleKeys) staleKeys.add(key)
    const oldServices = new Map(oldStack.services.map((service) => [service.serviceId, service]))
    const newServices = new Map(newStack.services.map((service) => [service.serviceId, service]))
    const serviceIds = new Set([...oldServices.keys(), ...newServices.keys()])
    const services = [...serviceIds].map((serviceId) => {
      const oldService = oldServices.get(serviceId)
      const newService = newServices.get(serviceId)
      if (!oldService && newService) return newService
      if (oldService && !newService) {
        for (const resource of oldService.resources) staleKeys.add(cleanupResourceKey(resource))
        return oldService
      }
      const merged = mergeResourceLists(oldService?.resources ?? [], newService?.resources ?? [])
      for (const key of merged.staleKeys) staleKeys.add(key)
      return {
        ...(oldService ?? newService!),
        ...(newService ?? {}),
        resources: merged.resources,
        estimatedReclaimableBytes: sumKnownResources(merged.resources),
        hasUnknownSize: merged.resources.some(itemHasUnknownSize),
      }
    })
    const stackResources = [...mergedOrphans.resources, ...services.flatMap((service) => service.resources)]
    stackGroups.push({
      ...oldStack,
      ...newStack,
      stackOrphans: mergedOrphans.resources,
      services,
      estimatedReclaimableBytes: sumKnownResources(stackResources),
      hasUnknownSize: stackResources.some(itemHasUnknownSize),
    })
  }

  const oldUnowned = previous.unownedGroup?.resources ?? []
  const newUnowned = partial.unownedGroup?.resources ?? []
  const mergedUnowned = mergeResourceLists(oldUnowned, newUnowned)
  for (const key of mergedUnowned.staleKeys) staleKeys.add(key)
  const unownedGroup =
    previous.unownedGroup || partial.unownedGroup
      ? {
          title: partial.unownedGroup?.title ?? previous.unownedGroup?.title ?? '未归属资源',
          resources: mergedUnowned.resources,
          estimatedReclaimableBytes: sumKnownResources(mergedUnowned.resources),
          hasUnknownSize: mergedUnowned.resources.some(itemHasUnknownSize),
        }
      : null

  const allResources = [
    ...stackGroups.flatMap((stack) => [...stack.stackOrphans, ...stack.services.flatMap((service) => service.resources)]),
    ...(unownedGroup?.resources ?? []),
  ]
  return {
    response: {
      ...previous,
      ...partial,
      status: 'ready',
      refreshing: true,
      scannedAt: partial.scannedAt ?? previous.scannedAt,
      serverDiskUsage: partial.serverDiskUsage ?? previous.serverDiskUsage,
      estimatedReclaimableBytes: sumKnownResources(allResources),
      hasUnknownSize: allResources.some(itemHasUnknownSize),
      stackGroups,
      unownedGroup,
      confirmationFingerprint: null,
    },
    staleKeys,
  }
}

export function projectResponseForPreset(pageScan: CleanupScanResponse, preset: CleanupPreset): CleanupScanResponse {
  const stackGroups: CleanupStackGroup[] = []
  let totalBytes = 0
  let totalUnknown = false

  for (const stack of pageScan.stackGroups) {
    const projectedOrphans = stack.stackOrphans.filter((item) => includesPreset(preset, item.minPreset))
    const projectedServices = stack.services
      .map((service) => {
        const resources = service.resources.filter((item) => includesPreset(preset, item.minPreset))
        const estimatedReclaimableBytes = resources.reduce(
          (sum, item) => sum + (item.estimatedReclaimableBytes ?? 0),
          0,
        )
        const hasUnknownSize = resources.some(itemHasUnknownSize)
        return {
          ...service,
          resources,
          estimatedReclaimableBytes,
          hasUnknownSize,
        }
      })
      .filter((service) => service.resources.length > 0)
    const orphanBytes = projectedOrphans.reduce((sum, item) => sum + (item.estimatedReclaimableBytes ?? 0), 0)
    const orphanUnknown = projectedOrphans.some(itemHasUnknownSize)
    const stackResources = [...projectedOrphans, ...projectedServices.flatMap((service) => service.resources)]
    const estimatedReclaimableBytes =
      orphanBytes + projectedServices.reduce((sum, service) => sum + service.estimatedReclaimableBytes, 0)
    const hasUnknownSize = orphanUnknown || projectedServices.some((service) => service.hasUnknownSize)
    if (stackResources.length > 0) {
      stackGroups.push({
        ...stack,
        stackOrphans: projectedOrphans,
        services: projectedServices,
        estimatedReclaimableBytes,
        hasUnknownSize,
      })
      totalBytes += estimatedReclaimableBytes
      totalUnknown = totalUnknown || hasUnknownSize
    }
  }

  const projectedUnowned =
    pageScan.unownedGroup?.resources.filter((item) => includesPreset(preset, item.minPreset)) ?? []
  const unownedBytes = projectedUnowned.reduce((sum, item) => sum + (item.estimatedReclaimableBytes ?? 0), 0)
  const unownedGroup =
    projectedUnowned.length > 0 && pageScan.unownedGroup
      ? {
          ...pageScan.unownedGroup,
          resources: projectedUnowned,
          estimatedReclaimableBytes: unownedBytes,
          hasUnknownSize: projectedUnowned.some(itemHasUnknownSize),
        }
      : null
  if (unownedGroup) {
    totalBytes += unownedBytes
    totalUnknown = totalUnknown || unownedGroup.hasUnknownSize
  }

  return {
    ...pageScan,
    preset,
    estimatedReclaimableBytes: totalBytes,
    hasUnknownSize: totalUnknown,
    stackGroups,
    unownedGroup,
  }
}
