import type { AuthRequiredDetails, SettingsResponse } from './api'

type TopbarAuthIdentitySource = {
  authorizationMode?: string | null
  currentGroups?: string[] | null
  currentUser?: string | null
  forwardHeaderName?: string | null
  groupHeaderName?: string | null
  matchedBy?: string | null
}

export type TopbarAuthIdentity = {
  triggerLabel: string
  currentUser: string
  currentGroups: string
  currentGroupsList: string[]
  authSource: string
  authorizationMode: string
  matchedBy: string
  forwardHeaderName: string
  groupHeaderName: string
}

const AUTH_SOURCE_LABEL = 'Forward Auth'

const AUTHORIZATION_MODE_LABELS: Record<string, string> = {
  group_only: '仅组',
  unconfigured: '未配置 allowlist',
  user_only: '仅用户',
  user_or_group: '用户或组任一命中',
}

const MATCHED_BY_LABELS: Record<string, string> = {
  anonymous_dev: '开发环境匿名',
  group: '用户组',
  user: '用户',
}

function trimOrNull(value: string | null | undefined): string | null {
  const trimmed = (value ?? '').trim()
  return trimmed ? trimmed : null
}

function normalizeGroups(groups: string[] | null | undefined): string[] {
  if (!Array.isArray(groups)) return []
  return groups
    .map((value) => value.trim())
    .filter(Boolean)
}

function displayOrDash(value: string | null | undefined): string {
  return trimOrNull(value) ?? '-'
}

function labelFromMap(raw: string | null, labels: Record<string, string>): string {
  if (!raw) return '-'
  return labels[raw] ?? raw
}

function resolveTriggerLabel(currentUser: string | null, currentGroups: string[], matchedBy: string | null): string {
  if (currentUser) return currentUser
  if (currentGroups.length > 0) return `组：${currentGroups[0]}`
  if (matchedBy === 'anonymous_dev') return '匿名开发'
  return AUTH_SOURCE_LABEL
}

function buildTopbarAuthIdentity(source?: TopbarAuthIdentitySource | null): TopbarAuthIdentity {
  const currentUser = trimOrNull(source?.currentUser)
  const currentGroupsList = normalizeGroups(source?.currentGroups)
  const authorizationMode = trimOrNull(source?.authorizationMode)
  const matchedBy = trimOrNull(source?.matchedBy)

  return {
    triggerLabel: resolveTriggerLabel(currentUser, currentGroupsList, matchedBy),
    currentUser: displayOrDash(currentUser),
    currentGroups: currentGroupsList.length > 0 ? currentGroupsList.join(', ') : '-',
    currentGroupsList,
    authSource: AUTH_SOURCE_LABEL,
    authorizationMode: labelFromMap(authorizationMode, AUTHORIZATION_MODE_LABELS),
    matchedBy: labelFromMap(matchedBy, MATCHED_BY_LABELS),
    forwardHeaderName: displayOrDash(source?.forwardHeaderName),
    groupHeaderName: displayOrDash(source?.groupHeaderName),
  }
}

export function buildTopbarAuthIdentityFromSettings(auth?: SettingsResponse['auth'] | null): TopbarAuthIdentity {
  return buildTopbarAuthIdentity(auth)
}

export function buildTopbarAuthIdentityFromAuthRequired(auth?: AuthRequiredDetails | null): TopbarAuthIdentity {
  return buildTopbarAuthIdentity(auth)
}

export function buildFallbackTopbarAuthIdentity(): TopbarAuthIdentity {
  return buildTopbarAuthIdentity(null)
}
