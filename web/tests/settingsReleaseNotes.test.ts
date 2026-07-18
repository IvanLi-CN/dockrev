import { describe, expect, test } from 'bun:test'

import type { SettingsResponse } from '../src/api'
import { buildSettingsSavePayload } from '../src/pages/settings/helpers'

function makeSettings(provider: SettingsResponse['releaseNotes']['provider']): SettingsResponse {
  return {
    backup: {
      enabled: true,
      requireSuccess: true,
      baseDir: '/tmp/dockrev-backups',
      skipTargetsOverBytes: 123,
    },
    resourceMonitor: {
      enabled: true,
      sampleIntervalSeconds: 60,
      retentionDays: 30,
    },
    schedules: {
      updateCheck: { enabled: false, cron: '*/30 * * * *' },
      ghcrWebhookAudit: { enabled: true, cron: '0 3 * * *' },
    },
    releaseNotes: {
      provider,
      octoRill: {
        enabled: true,
        apiBaseUrl: 'https://octo.example.com/octo-rill',
        apiKeyMasked: '••••••••••••••••',
        apiKey: '••••••••••••••••',
        defaultView: 'smart',
      },
    },
    auth: {
      forwardHeaderName: 'x-forwarded-user',
      groupHeaderName: 'x-forwarded-groups',
      allowAnonymousInDev: false,
      authorizationMode: 'headers',
      currentGroups: [],
    },
    instance: {
      publicBaseUrl: 'https://dockrev.example.com/',
    },
  }
}

describe('settings release notes payload', () => {
  test('saves the selected provider and keeps masked octoRill api keys unchanged', () => {
    const payload = buildSettingsSavePayload(makeSettings('octoRill'))

    expect(payload.releaseNotes?.provider).toBe('octoRill')
    expect(payload.releaseNotes?.octoRill).toEqual({
      apiBaseUrl: 'https://octo.example.com/octo-rill',
      defaultView: 'smart',
    })
  })

  test('keeps GitHub as the explicit runtime provider when OctoRill config still exists', () => {
    const payload = buildSettingsSavePayload(makeSettings('gitHub'))

    expect(payload.releaseNotes?.provider).toBe('gitHub')
    expect(payload.releaseNotes?.octoRill?.defaultView).toBe('smart')
  })
})
