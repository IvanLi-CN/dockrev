import { describe, expect, test } from 'bun:test'

import type { StackDetail, StackListItem } from '../src/api'
import { withCollapseDefaults } from '../src/pages/overviewHelpers'

function makeService(
  overrides: Partial<StackDetail['services'][number]>,
): StackDetail['services'][number] {
  return {
    id: 'svc-default',
    name: 'default',
    image: {
      ref: 'ghcr.io/acme/default:latest',
      tag: 'latest',
      digest: null,
      resolvedTag: null,
      resolvedTags: null,
    },
    candidate: null,
    ignore: null,
    versionInference: { status: 'ready', reason: null, checkedAt: null },
    settings: {
      autoRollback: true,
      backupTargets: { bindPaths: {}, volumeNames: {} },
    },
    archived: false,
    ...overrides,
  }
}

describe('withCollapseDefaults', () => {
  test('expands stacks that have non-ok candidate rows even when stack.updates is 0', () => {
    const stacks: StackListItem[] = [
      {
        id: 'stack-infra',
        name: 'infra',
        status: 'healthy',
        services: 2,
        updates: 0,
        lastCheckAt: null,
      },
    ]
    const details: Record<string, StackDetail> = {
      'stack-infra': {
        id: 'stack-infra',
        name: 'infra',
        compose: { type: 'path', composeFiles: ['/srv/infra/compose.yml'], envFile: null },
        services: [
          makeService({
            id: 'svc-loki',
            name: 'loki',
            candidate: {
              tag: '2.9.1',
              resolvedTag: null,
              digest: null,
              archMatch: 'unknown',
              arch: ['linux/amd64'],
            },
          }),
          makeService({
            id: 'svc-prom',
            name: 'prometheus',
            candidate: {
              tag: '2.50.0',
              resolvedTag: null,
              digest: null,
              archMatch: 'mismatch',
              arch: ['linux/arm64'],
            },
          }),
        ],
      },
    }

    const collapsed = withCollapseDefaults({}, stacks, details, 'all')

    expect(collapsed['stack-infra']).toBe(false)
  })
})
