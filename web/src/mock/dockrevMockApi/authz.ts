import type { SettingsResponse } from '../../api'
import type { DockrevMockAuthz } from './shared'

/**
 * The backend exempts exactly these reads from authorization
 * (`crates/dockrev-api/src/api/mod.rs`). The mock must match, otherwise a
 * blocked visitor sees a mock artefact instead of the product's 200 contract.
 */
function isPublicMockApiPath(urlPath: string, method: string): boolean {
  if (method === 'GET' && (urlPath === '/api/health' || urlPath === '/api/version')) return true
  return urlPath.startsWith('/api/webhooks/')
}

const REASON_MESSAGE: Record<
  Exclude<DockrevMockAuthz, { state: 'allowed' }>['reason'],
  string
> = {
  authz_no_match: 'forward auth identity is not allowed',
  identity_missing: 'forward auth identity is missing',
  authz_config_missing: 'authorization target is not configured',
}

/**
 * Builds the same `401 auth_required` payload the Rust `authz_error` handler
 * emits, so the product's own unauthorized page takes over unchanged.
 * Returns null when the request is allowed.
 */
export function mockAuthzRejection(
  authz: DockrevMockAuthz | undefined,
  urlPath: string,
  method: string,
  auth: SettingsResponse['auth'] | undefined,
): { status: 401; body: unknown } | null {
  if (authz?.state !== 'blocked' || isPublicMockApiPath(urlPath, method)) return null
  return {
    status: 401,
    body: {
      error: {
        code: 'auth_required',
        message: 'auth required',
        details: {
          reason: authz.reason,
          message: REASON_MESSAGE[authz.reason],
          forwardHeaderName: auth?.forwardHeaderName ?? 'X-Forwarded-User',
          groupHeaderName: auth?.groupHeaderName ?? 'Remote-Groups',
          allowedUserMasked: auth?.allowedUserMasked ?? null,
          allowedGroupMasked: auth?.allowedGroupMasked ?? null,
          currentUser: auth?.currentUser ?? null,
          currentGroups: auth?.currentGroups ?? [],
          avatarUrl: null,
          authorizationMode: auth?.authorizationMode ?? 'user_or_group',
        },
      },
    },
  }
}
