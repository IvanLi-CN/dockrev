function normalize(value: string | null | undefined): string {
  const trimmed = (value ?? '').trim()
  return trimmed.length > 0 ? trimmed : '-'
}

export function formatJobTypeLabel(type: string): string {
  const raw = normalize(type)
  if (raw === '-') return raw
  if (raw === 'check') return '检查任务'
  if (raw === 'discovery') return '发现扫描'
  if (raw === 'runtime_scan') return '运行时扫描'
  if (raw === 'github_packages_webhook') return 'GHCR Webhook'
  if (raw === 'update') return '更新任务'
  if (raw === 'rollback') return '回滚任务'
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

export function formatJobMachineName(type: string, scope: string): string {
  const normalizedType = normalize(type)
  const normalizedScope = normalize(scope)
  if (normalizedType === '-' && normalizedScope === '-') return '-'
  if (normalizedScope === '-') return normalizedType
  if (normalizedType === '-') return normalizedScope
  return `${normalizedType}.${normalizedScope}`
}
