import type { Meta, StoryObj } from '@storybook/react'
import { DetailRouteServiceTree } from '../../components/DetailRouteServiceTree'
import { SERVICE_TREE_REFRESH_EVENT } from '../../serviceTreeRefresh'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'
import { expectStory, waitForCondition } from '../pages/storyAssertions'

const route = { name: 'service', stackId: 'stack-prod', serviceId: 'svc-prod-api', section: 'overview' } as const

const meta: Meta<typeof DetailRouteServiceTree> = {
  title: 'Components/DetailRouteServiceTree',
  component: DetailRouteServiceTree,
  decorators: [withDockrevMockApi],
  tags: ['autodocs'],
  parameters: { dockrevApiScenario: 'dashboard-demo' },
}

export default meta
type Story = StoryObj<typeof DetailRouteServiceTree>

function render(variant: 'desktop' | 'mobile') {
  return () => (
    <div style={{ maxWidth: variant === 'mobile' ? 390 : 430, padding: 24 }}>
      <DetailRouteServiceTree route={route} variant={variant} />
    </div>
  )
}

export const RuntimeStateMatrix: Story = {
  render: render('desktop'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('.detailRouteServiceLink')))
    const toggles = canvasElement.querySelectorAll<HTMLButtonElement>('.detailRouteStackToggle')
    toggles[1]?.click()
    await waitForCondition(() => canvasElement.querySelectorAll('.detailRouteStatusDotLifecycle-running, .detailRouteStatusDotLifecycle-partial, .detailRouteStatusDotLifecycle-stopped, .detailRouteStatusDotLifecycle-unknown').length >= 6)
    const dots = canvasElement.querySelectorAll('.detailRouteStatusDotLifecycle-running, .detailRouteStatusDotLifecycle-partial, .detailRouteStatusDotLifecycle-stopped, .detailRouteStatusDotLifecycle-unknown')
    expectStory(dots.length >= 6, 'service tree should show lifecycle states across expanded stacks')
    expectStory(Boolean(canvasElement.querySelector('.detailRouteServiceUpdateDot')), 'updatable version should show a signal dot')
    expectStory(Boolean(canvasElement.querySelector('.detailRouteServiceLinkActive')), 'active service should remain highlighted')
    const debug = globalThis.__DOCKREV_MOCK_DEBUG__
    await waitForCondition(() => Number(debug?.stackDetailCallsById?.['stack-prod'] ?? 0) >= 1)
    const beforeRefresh = Number(debug?.stackDetailCallsById?.['stack-prod'] ?? 0)
    window.dispatchEvent(new CustomEvent(SERVICE_TREE_REFRESH_EVENT, { detail: { stackId: 'stack-prod', reason: 'storybook-immediate-refresh' } }))
    await waitForCondition(() => Number(debug?.stackDetailCallsById?.['stack-prod'] ?? 0) > beforeRefresh)
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' })
    const beforeHiddenResume = Number(debug?.stackDetailCallsById?.['stack-prod'] ?? 0)
    document.dispatchEvent(new Event('visibilitychange'))
    await new Promise((resolve) => setTimeout(resolve, 40))
    expectStory(Number(debug?.stackDetailCallsById?.['stack-prod'] ?? 0) === beforeHiddenResume, 'hidden detail pages should pause refresh')
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' })
    document.dispatchEvent(new Event('visibilitychange'))
    await waitForCondition(() => Number(debug?.stackDetailCallsById?.['stack-prod'] ?? 0) > beforeHiddenResume)
  },
}

export const RuntimeStateMatrixMobile: Story = {
  parameters: { viewport: { defaultViewport: 'mobile1' } },
  render: render('mobile'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('.detailRouteServiceLink')))
    const rows = canvasElement.querySelectorAll('.detailRouteServiceLink')
    expectStory(rows.length > 0, 'mobile service tree should render service rows')
    expectStory(Array.from(rows).every((row) => row.getBoundingClientRect().height >= 40), 'mobile service rows should keep the 40px touch target')
  },
}
