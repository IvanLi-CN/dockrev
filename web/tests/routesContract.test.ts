import { describe, expect, test } from 'bun:test'
import { isSafeDynamicSegment, matchesContractPage } from '../src/routeContract'
import { parseRoute } from '../src/routes'

describe('strict page route contract', () => {
  test('accepts the documented fixed and dynamic pages', () => {
    expect(matchesContractPage('/')).toBe(true)
    expect(matchesContractPage('/queue/job_01-abc')).toBe(true)
    expect(matchesContractPage('/services/stack-prod/svc-prod-api/logs')).toBe(true)
  })

  test('rejects unsafe segments and unknown paths', () => {
    expect(isSafeDynamicSegment('job_01-abc')).toBe(true)
    expect(isSafeDynamicSegment('../etc')).toBe(false)
    expect(isSafeDynamicSegment('ümlaut')).toBe(false)
    expect(matchesContractPage('/apple-touch-icon.png')).toBe(false)
    expect(matchesContractPage('/made-up-deep-link')).toBe(false)
  })

  test('does not turn unknown paths into overview', () => {
    expect(parseRoute('/made-up-deep-link')).toEqual({ name: 'not-found', pathname: '/made-up-deep-link' })
    expect(parseRoute('/queue/../../etc')).toEqual({ name: 'not-found', pathname: '/queue/../../etc' })
  })
})
