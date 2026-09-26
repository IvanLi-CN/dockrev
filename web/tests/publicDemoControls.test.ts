import { describe, expect, test } from 'bun:test'

import { buildPublicDemoSeedFixture } from '../src/demo/publicDemoControls'

describe('selected version updates public demo fixture', () => {
  test('keeps the configured image tag floating while showing the deployed version', () => {
    const fixture = buildPublicDemoSeedFixture('service-selected-version-updates')
    const service = fixture.stackById['stack-prod']?.services.find((item) => item.id === 'svc-prod-api')

    expect(service?.image.ref).toBe('ghcr.io/acme/api:latest')
    expect(service?.image.tag).toBe('latest')
    expect(service?.image.resolvedTag).toBe('5.2.1')
    expect(fixture.rollbackTargetByServiceId['svc-prod-api']?.available).toBe(true)
  })
})
