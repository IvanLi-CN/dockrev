import { describe, expect, test } from 'bun:test'
import { readFileSync } from 'node:fs'

const READ_SURFACE_STATE_HOOKS = [
  'useOverviewPageState.tsx',
  'useServiceDetailPageState.tsx',
  'useServicesPageState.tsx',
] as const

function readHookSource(fileName: (typeof READ_SURFACE_STATE_HOOKS)[number]) {
  return readFileSync(new URL(`../src/pages/${fileName}`, import.meta.url), 'utf8')
}

describe('runtime scan page-open policy', () => {
  test('read surfaces do not enqueue runtime scans on mount', () => {
    for (const fileName of READ_SURFACE_STATE_HOOKS) {
      expect(readHookSource(fileName)).not.toContain('triggerRuntimeScan')
    }
  })
})
