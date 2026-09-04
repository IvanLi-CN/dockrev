import { stripAppBase } from './appBase'

export const DYNAMIC_SEGMENT_PATTERN = '[A-Za-z0-9][A-Za-z0-9_-]{0,127}'
const dynamicSegment = new RegExp(`^(?:${DYNAMIC_SEGMENT_PATTERN})$`)

export const STATIC_PAGE_PATHS = [
  '/',
  '/queue',
  '/queue/version-inference',
  '/queue/ghcr-webhooks',
  '/queue/ghcr-webhook-inbox',
  '/settings/ghcr-webhooks',
  '/services',
  '/cleanup',
  '/version-inference',
  '/deploy-check',
  '/settings',
  '/settings/account',
  '/settings/maintenance',
  '/settings/backup',
  '/settings/monitoring',
  '/settings/schedules',
  '/settings/release-notes',
  '/settings/notifications',
  '/settings/integrations',
] as const

export const DYNAMIC_PAGE_TEMPLATES = [
  '/queue/:jobId',
  '/services/:stackId',
  '/services/:stackId/:serviceId',
  '/services/:stackId/:serviceId/overview',
  '/services/:stackId/:serviceId/versions',
  '/services/:stackId/:serviceId/history',
  '/services/:stackId/:serviceId/monitoring',
  '/services/:stackId/:serviceId/backup',
  '/services/:stackId/:serviceId/logs',
  '/services/:stackId/:serviceId/settings',
] as const

export const RESERVED_PREFIXES = ['/api', '/supervisor', '/assets'] as const

export type RouteContract = {
  version: 1
  basePath: string
  dynamicSegmentPattern: string
  staticPagePaths: readonly string[]
  dynamicPageTemplates: readonly string[]
  reservedPrefixes: readonly string[]
}

export function buildRouteContract(basePath = '/'): RouteContract {
  return {
    version: 1,
    basePath,
    dynamicSegmentPattern: DYNAMIC_SEGMENT_PATTERN,
    staticPagePaths: STATIC_PAGE_PATHS,
    dynamicPageTemplates: DYNAMIC_PAGE_TEMPLATES,
    reservedPrefixes: RESERVED_PREFIXES,
  }
}

export function isSafeDynamicSegment(value: string): boolean {
  return dynamicSegment.test(value)
}

export function contractPath(pathname: string): string {
  const path = stripAppBase(pathname)
  if (path === '/') return '/'
  return path.replace(/\/+$/, '') || '/'
}

export function matchesContractPage(pathname: string): boolean {
  const path = contractPath(pathname)
  if ((STATIC_PAGE_PATHS as readonly string[]).includes(path)) return true
  return DYNAMIC_PAGE_TEMPLATES.some((template) => matchesTemplate(template, path))
}

function matchesTemplate(template: string, path: string): boolean {
  const templateParts = template.split('/').filter(Boolean)
  const pathParts = path.split('/').filter(Boolean)
  return templateParts.length === pathParts.length && templateParts.every((part, index) => (
    part.startsWith(':') ? isSafeDynamicSegment(pathParts[index] ?? '') : part === pathParts[index]
  ))
}

export function isReservedPath(pathname: string): boolean {
  const path = contractPath(pathname)
  return RESERVED_PREFIXES.some((prefix) => path === prefix || path.startsWith(`${prefix}/`))
}
