import { describe, expect, test } from 'bun:test'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const webRoot = resolve(import.meta.dir, '..')
const stylesheet = readFileSync(resolve(webRoot, 'src/App.css'), 'utf8')

function readSource(fileName: string) {
  return readFileSync(resolve(webRoot, 'src/pages', fileName), 'utf8')
}

function readRule(className: string) {
  const match = stylesheet.match(new RegExp(`\\.${className}\\s*\\{([^}]*)\\}`))
  expect(match).not.toBeNull()
  return match?.[1] ?? ''
}

function expectColumnGap(className: string, gap: string) {
  const declarations = readRule(className)
  expect(declarations).toMatch(/\bdisplay:\s*flex;/)
  expect(declarations).toMatch(/\bflex-direction:\s*column;/)
  expect(declarations).toContain(`gap: ${gap};`)
}

describe('async data region layout rhythm', () => {
  test('binds every affected page to a local layout region', () => {
    const bindings = [
      ['SettingsPage.tsx', 'settingsCoreRegion'],
      ['ServiceDetailPage.tsx', 'serviceDetailSettingsRegion'],
      ['VersionInferencePage.tsx', 'versionInferenceDataRegion'],
      ['GhcrWebhookInboxPage.tsx', 'ghcrInboxDataRegion'],
      ['StackDetailPage.tsx', 'stackDetailData'],
      ['DeployWelcomePage.tsx', 'deployWelcomeAsyncRegion'],
    ] as const

    for (const [fileName, className] of bindings) {
      expect(readSource(fileName)).toContain(`className="${className}"`)
    }
  })

  test('uses the established local vertical rhythm for each region', () => {
    expectColumnGap('settingsCoreRegion', '16px')
    expectColumnGap('serviceDetailSettingsRegion', '16px')
    expectColumnGap('versionInferenceDataRegion', '14px')
    expectColumnGap('ghcrInboxDataRegion', '12px')
    expectColumnGap('stackDetailData', '16px')
    expectColumnGap('deployWelcomeAsyncRegion', '18px')
  })

  test('keeps AsyncDataRegion layout-neutral and its overlay absolute', () => {
    const asyncDataRegion = readRule('asyncDataRegion')
    expect(asyncDataRegion).not.toMatch(/\b(?:display|gap|margin(?:-[a-z]+)?)\s*:/)
    expect(stylesheet).toMatch(/\.asyncDataOverlay\s*\{[^}]*position:\s*absolute;/s)
  })

  test('removes legacy sibling offsets that would double the local gap', () => {
    expect(stylesheet).not.toMatch(/\.svcComposeCard\s*\{/)
    expect(readRule('serviceSafeguardCard')).not.toMatch(/\bmargin(?:-[a-z]+)?\s*:/)
    expect(readSource('StackDetailPage.tsx')).not.toContain('style={{ marginTop: 16 }}')
  })
})
