import { describe, expect, test } from 'bun:test'
import { renderToStaticMarkup } from 'react-dom/server'

import { isAnonymousPublicApiRequest } from '../src/api'
import { UnauthorizedPage } from '../src/pages/UnauthorizedPage'

describe('auth boundary helpers', () => {
  test('keeps only health/version/webhooks in the anonymous public set', () => {
    expect(isAnonymousPublicApiRequest('/api/health')).toBe(true)
    expect(isAnonymousPublicApiRequest('/api/version')).toBe(true)
    expect(isAnonymousPublicApiRequest('/api/webhooks/trigger', 'POST')).toBe(true)
    expect(isAnonymousPublicApiRequest('/api/webhooks/github-packages', 'POST')).toBe(true)

    expect(isAnonymousPublicApiRequest('/api/deploy-check/report')).toBe(false)
    expect(isAnonymousPublicApiRequest('/api/settings')).toBe(false)
    expect(isAnonymousPublicApiRequest('/supervisor/health')).toBe(false)
  })

  test('unauthorized page no longer offers a deploy-check fallback action', () => {
    const html = renderToStaticMarkup(
      <UnauthorizedPage
        authDetails={{
          reason: 'identity_missing',
          forwardHeaderName: 'Remote-User',
          groupHeaderName: 'Remote-Groups',
          allowedGroupMasked: 'dockrev-users',
          currentGroups: [],
        }}
      />,
    )

    expect(html).toContain('当前身份未获 Dockrev 授权')
    expect(html).toContain('重新加载')
    expect(html).not.toContain('打开自检页')
    expect(html).not.toContain('/deploy-check')
  })
})
