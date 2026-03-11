import { describe, expect, test } from 'bun:test'

import { imageRepoFromImageRef } from '../src/imageRepo'

describe('imageRepoFromImageRef', () => {
  test('parses repo:tag', () => {
    expect(imageRepoFromImageRef('ghcr.io/org/app:latest')).toBe('ghcr.io/org/app')
  })

  test('parses repo:tag@digest', () => {
    expect(imageRepoFromImageRef('ghcr.io/org/app:latest@sha256:deadbeef')).toBe('ghcr.io/org/app')
  })

  test('parses repo@digest (digest-only)', () => {
    expect(imageRepoFromImageRef('ghcr.io/org/app@sha256:deadbeef')).toBe('ghcr.io/org/app')
  })

  test('parses docker hub library repo:tag', () => {
    expect(imageRepoFromImageRef('alpine:3.19')).toBe('docker.io/library/alpine')
  })

  test('parses docker hub library repo@digest', () => {
    expect(imageRepoFromImageRef('alpine@sha256:deadbeef')).toBe('docker.io/library/alpine')
  })

  test('parses digest-only refs with a registry port', () => {
    expect(imageRepoFromImageRef('localhost:5000/acme/demo@sha256:deadbeef')).toBe('localhost:5000/acme/demo')
  })

  test('parses digest-only refs for localhost registry', () => {
    expect(imageRepoFromImageRef('localhost/acme/demo@sha256:deadbeef')).toBe('localhost/acme/demo')
  })

  test('returns null for tagless refs without a digest', () => {
    expect(imageRepoFromImageRef('ghcr.io/org/app')).toBe(null)
  })
})

