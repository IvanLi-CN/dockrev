import { describe, expect, test } from 'bun:test'
import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { renderToStaticMarkup } from 'react-dom/server'

import { JobDetailErrorIllustrationAsset } from '../src/components/jobDetailErrorIllustration/JobDetailErrorIllustrationAsset'

describe('JobDetailErrorIllustrationAsset', () => {
  test('renders the Apache-licensed Adobe Spectrum illustration with the canonical geometry', async () => {
    const html = renderToStaticMarkup(<JobDetailErrorIllustrationAsset className="asyncDataInitialErrorIllustration" />)
    const source = await readFile(new URL('../src/components/jobDetailErrorIllustration/JobDetailErrorIllustrationAsset.tsx', import.meta.url), 'utf8')

    expect(html).toContain('data-illustration-source="adobe-spectrum-error"')
    expect(html).toContain('aria-hidden="true"')
    expect(html).toContain('class="asyncDataInitialErrorIllustration"')
    expect(html).toContain('viewBox="0 0 146.569 94"')
    expect(source).toContain('M415.8 2988v-18a2.006 2.006 0 012-2h118')
    expect(source).toContain('M413.1 2995.5l-18.4 31.1a2.916 2.916 0 002.6 4.4')
    expect(source).toContain('transform="translate(-392.731 -2966.5)"')
  })

  test('uses only transparent source-vector primitives and host-controlled color tokens', async () => {
    const source = await readFile(new URL('../src/components/jobDetailErrorIllustration/JobDetailErrorIllustrationAsset.tsx', import.meta.url), 'utf8')
    const license = await readFile(new URL('../src/components/jobDetailErrorIllustration/assets/SPECTRUM-APACHE-2.0-LICENSE.txt', import.meta.url))

    expect(source).toContain('var(--job-detail-error-illustration-primary)')
    expect(source).toContain('var(--job-detail-error-illustration-error)')
    expect(source).not.toMatch(/<(?:image|foreignObject|filter|mask|clipPath|rect)\b/)
    expect(source).not.toMatch(/(?:png|jpe?g|webp|data:image)/i)
    expect(createHash('sha256').update(license).digest('hex')).toBe(
      '7dfe6526888bac51759c99f9a51262ba2711a8c12a067f2181609dd9a4066b84',
    )
  })
})
