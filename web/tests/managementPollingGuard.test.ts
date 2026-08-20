import { describe, expect, test } from 'bun:test'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const managementModules = [
  'src/App.tsx',
  'src/pages/OverviewPage.tsx',
  'src/pages/QueuePage.tsx',
  'src/pages/ServicesPage.tsx',
  'src/pages/CleanupPage.tsx',
  'src/pages/SettingsPage.tsx',
  'src/pages/StackDetailPage.tsx',
  'src/pages/ServiceDetailPage.tsx',
  'src/pages/JobDetailPage.tsx',
  'src/pages/useOverviewPageState.tsx',
  'src/pages/useArchivedStacksState.ts',
  'src/pages/useServicesPageState.tsx',
  'src/pages/useServiceDetailPageState.tsx',
  'src/digestInferenceTracker.ts',
  'src/components/CurrentVersionPopover.tsx',
  'src/components/VersionTagsPopover.tsx',
]

describe('management polling guard', () => {
  test('uses the application event stream without management refresh intervals or local event sources', () => {
    for (const module of managementModules) {
      const source = readFileSync(resolve(import.meta.dir, '..', module), 'utf8')
      expect(source).not.toContain('setInterval(')
      expect(source).not.toContain('newJobsEventsSource(')
      expect(source).not.toContain('newVersionInferenceEventsSource(')
      expect(source).not.toContain('newGitHubPackagesWebhookDeliveriesEventsSource(')
      expect(source).not.toContain('pollTracked(')
    }
  })

  test('owns exactly one application-level management EventSource', () => {
    const source = readFileSync(resolve(import.meta.dir, '..', 'src/managementEvents.tsx'), 'utf8')
    expect(source.match(/new EventSource\(/g)).toHaveLength(1)
  })

  test('refreshes archived entities from management events instead of a local refresh timer', () => {
    const source = readFileSync(resolve(import.meta.dir, '..', 'src/pages/useArchivedStacksState.ts'), 'utf8')
    expect(source).toContain('useManagementEventBatch')
    expect(source).not.toContain('setTimeout(')
  })

  test('refreshes stack lists from complete REST snapshots with the guarded request path', () => {
    const overview = readFileSync(resolve(import.meta.dir, '..', 'src/pages/useOverviewPageState.tsx'), 'utf8')
    const services = readFileSync(resolve(import.meta.dir, '..', 'src/pages/useServicesPageState.tsx'), 'utf8')
    expect(overview).toContain('requestRefresh({ domains })')
    expect(overview).toContain('setStacks(nextStacks)')
    expect(services).toContain('setStacks(nextStacks)')
    expect(services).toContain('setArchivedStacks(nextArchived)')
    expect(overview).not.toContain('prev.map((item) => byId.get(item.id) ?? item)')
  })
})
