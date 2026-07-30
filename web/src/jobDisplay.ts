function normalize(value: string | null | undefined): string {
  const trimmed = (value ?? '').trim()
  return trimmed.length > 0 ? trimmed : '-'
}

export type JobReadableDisplay = {
  primaryLabel: string
  scopeTag: string | null
  typeTone: JobTypeTone
}

export type JobTypeTone = 'check' | 'cleanup' | 'discovery' | 'runtimeScan' | 'ghcrWebhook' | 'update' | 'rollback' | 'default'

export function formatJobTypeLabel(type: string): string {
  const raw = normalize(type)
  if (raw === '-') return raw
  if (raw === 'check') return '检查任务'
  if (raw === 'cleanup_apply') return '清理任务'
  if (raw === 'discovery') return '发现扫描'
  if (raw === 'runtime_scan') return '运行时扫描'
  if (raw === 'github_packages_webhook') return 'GHCR Webhook'
  if (raw === 'github_packages_webhook_sync_all') return 'GHCR 全量同步'
  if (raw === 'github_packages_webhook_sync_repo') return 'GHCR 单仓库同步'
  if (raw === 'repo_link_backfill') return '仓库链接补齐'
  if (raw === 'update') return '更新任务'
  if (raw === 'rollback') return '回滚任务'
  if (raw === 'service_lifecycle') return '服务生命周期'
  return raw
}

export function formatJobScopeLabel(scope: string): string {
  const raw = normalize(scope)
  if (raw === '-') return raw
  if (raw === 'all') return '全局'
  if (raw === 'stack') return 'Stack'
  if (raw === 'service') return '服务'
  return raw
}

export function formatJobReadableName(type: string, scope: string): string {
  const typeLabel = formatJobTypeLabel(type)
  const scopeLabel = formatJobScopeLabel(scope)
  if (typeLabel === '-' && scopeLabel === '-') return '-'
  if (scopeLabel === '-') return typeLabel
  if (typeLabel === '-') return scopeLabel
  return `${typeLabel} · ${scopeLabel}`
}

export function formatJobReadableDisplay(type: string, scope: string): JobReadableDisplay {
  const rawType = normalize(type)
  const typeLabel = formatJobTypeLabel(type)
  const scopeLabel = formatJobScopeLabel(scope)
  const typeTone = resolveJobTypeTone(rawType)
  if (typeLabel === '-' && scopeLabel === '-') return { primaryLabel: '-', scopeTag: null, typeTone: 'default' }
  if (scopeLabel === '-') return { primaryLabel: typeLabel, scopeTag: null, typeTone }
  if (typeLabel === '-') return { primaryLabel: scopeLabel, scopeTag: null, typeTone: 'default' }
  return { primaryLabel: typeLabel, scopeTag: scopeLabel, typeTone }
}

export function formatJobMachineName(type: string, scope: string): string {
  const normalizedType = normalize(type)
  const normalizedScope = normalize(scope)
  if (normalizedType === '-' && normalizedScope === '-') return '-'
  if (normalizedScope === '-') return normalizedType
  if (normalizedType === '-') return normalizedScope
  return `${normalizedType}.${normalizedScope}`
}

function resolveJobTypeTone(type: string): JobTypeTone {
  if (type === 'check') return 'check'
  if (type === 'cleanup_apply') return 'cleanup'
  if (type === 'discovery') return 'discovery'
  if (type === 'runtime_scan') return 'runtimeScan'
  if (type === 'github_packages_webhook') return 'ghcrWebhook'
  if (type === 'github_packages_webhook_sync_all') return 'ghcrWebhook'
  if (type === 'github_packages_webhook_sync_repo') return 'ghcrWebhook'
  if (type === 'repo_link_backfill') return 'discovery'
  if (type === 'update') return 'update'
  if (type === 'rollback') return 'rollback'
  if (type === 'service_lifecycle') return 'update'
  return 'default'
}
