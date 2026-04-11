import { describe, expect, test } from 'bun:test'
import { renderToStaticMarkup } from 'react-dom/server'

import { DiscoveryHistoryPopover } from '../src/components/DiscoveryHistoryPopover'

describe('DiscoveryHistoryPopover', () => {
  test('renders a single pill trigger for the default variant', () => {
    const html = renderToStaticMarkup(
      <DiscoveryHistoryPopover serviceId="svc-pill" count={3} />,
    )

    expect(html).toContain('aria-label="发现 3 次，查看版本时间线"')
    expect(html).toContain('>发现 3 次</button>')
    expect(html).not.toContain('discoveryHistoryTimelineTrigger')
  })

  test('renders nothing when the count is absent', () => {
    const html = renderToStaticMarkup(
      <DiscoveryHistoryPopover serviceId="svc-empty" count={0} />,
    )

    expect(html).toBe('')
  })
})
