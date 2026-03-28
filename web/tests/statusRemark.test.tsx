import { describe, expect, test } from 'bun:test'
import { renderToStaticMarkup } from 'react-dom/server'

import type { Service } from '../src/api'
import { StatusRemark } from '../src/ui'
import { serviceRowStatus } from '../src/updateStatus'

function makeService(overrides?: Partial<Service>): Service {
  return {
    id: 'svc-1',
    name: 'svc-1',
    image: {
      ref: 'ghcr.io/acme/demo:latest',
      tag: 'latest',
      digest: 'sha256:current',
      resolvedTag: 'v1.0.0',
      resolvedTags: ['v1.0.0'],
    },
    candidate: {
      tag: 'latest',
      resolvedTag: 'v1.1.0',
      digest: 'sha256:candidate',
      archMatch: 'match',
      arch: ['linux/amd64'],
    },
    ignore: null,
    versionInference: { status: 'ready', reason: null, checkedAt: null },
    settings: {
      autoRollback: true,
      backupTargets: {
        bindPaths: {},
        volumeNames: {},
      },
    },
    archived: false,
    ...overrides,
  }
}

describe('StatusRemark discovery count', () => {
  test('renders the discovery count as a compact badge when a service has historical discoveries', () => {
    const service = makeService({ newVersionDiscoveryCount: 3 })
    const html = renderToStaticMarkup(
      <StatusRemark service={service} status={serviceRowStatus(service)} />,
    )

    expect(html).toContain('可更新')
    expect(html).toContain('statusColHasCompactBadge')
    expect(html).toContain('discoveryHistoryTriggerCompact')
    expect(html).toContain('aria-label="发现 3 次，查看版本时间线"')
    expect(html).toContain('>3</button>')
    expect(html).not.toContain('>发现 3 次</button>')
    expect(html).toContain('<button')
  })

  test('omits the discovery count pill when the count is absent', () => {
    const service = makeService({ newVersionDiscoveryCount: null })
    const html = renderToStaticMarkup(
      <StatusRemark service={service} status={serviceRowStatus(service)} />,
    )

    expect(html).toContain('可更新')
    expect(html).not.toContain('发现')
  })
})
