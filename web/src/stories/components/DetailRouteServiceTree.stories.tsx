import type { Meta, StoryObj } from '@storybook/react'
import { fireEvent, userEvent, within } from 'storybook/test'
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

export const ServiceContextMenuRunning: Story = {
  render: render('desktop'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('.detailRouteServiceLink')))
    const toggle = canvasElement.querySelector<HTMLButtonElement>('.detailRouteStackToggle')!
    fireEvent.contextMenu(toggle)
    await new Promise((resolve) => setTimeout(resolve, 40))
    expectStory(!document.body.querySelector('[role="menu"]'), 'stack expand arrow must not trigger the context menu')
    const service = Array.from(canvasElement.querySelectorAll<HTMLAnchorElement>('.detailRouteServiceLink'))
      .find((row) => row.textContent?.includes('api'))
    expectStory(Boolean(service), 'running service row should exist')
    fireEvent.contextMenu(service!)
    const body = within(document.body)
    await waitForCondition(() => Boolean(body.queryByRole('menu')))
    await waitForCondition(() => Boolean(body.queryByText('重启')))
    const labels = body.getAllByRole('menuitem').map((item) => item.textContent?.trim())
    expectStory(labels.join('|') === '重启|停止|更新', 'running menu should keep restart, stop, separator, update order')
    const updateItem = body.getByText('更新').closest('[role="menuitem"]')
    expectStory(Boolean(updateItem?.querySelector('svg[data-lucide="download"]')), 'update action should use the same download icon as the service detail action')
    await userEvent.click(body.getByText('重启'))
    await waitForCondition(() => globalThis.__DOCKREV_MOCK_DEBUG__?.lastLifecycleRequest?.action === 'restart')
    expectStory(globalThis.__DOCKREV_MOCK_DEBUG__?.lastLifecycleRequest?.id === 'svc-prod-api', 'restart should submit the selected service directly')
    await waitForCondition(() => Boolean(body.queryByText('查看任务')))

    fireEvent.contextMenu(service!)
    await waitForCondition(() => Boolean(body.queryByRole('menu')))
    await waitForCondition(() => Boolean(body.queryByText('更新')))
    await userEvent.click(body.getByText('更新'))
    await waitForCondition(() => Boolean(globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest))
    const update = globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest as Record<string, unknown>
    expectStory(update.scope === 'service', 'service context update should remain service-scoped')
    expectStory(update.backupMode === 'inherit', 'service context update should inherit backup policy')
  },
}

export const ServiceContextMenuKeyboardStopped: Story = {
  render: render('desktop'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.querySelectorAll('.detailRouteServiceLink').length >= 3)
    const service = Array.from(canvasElement.querySelectorAll<HTMLAnchorElement>('.detailRouteServiceLink'))
      .find((row) => row.textContent?.includes('worker'))
    expectStory(Boolean(service), 'stopped service row should exist')
    service!.focus()
    await userEvent.keyboard('{Shift>}{F10}{/Shift}')
    const body = within(document.body)
    await waitForCondition(() => Boolean(body.queryByText('启动')))
    expectStory(!body.queryByText('重启'), 'stopped menu must replace restart with start')
    expectStory(!body.queryByText('停止'), 'stopped menu must omit stop')
    expectStory(Boolean(body.getByText('更新').closest('[data-disabled]')), 'ignored update should expose a disabled item')
    await userEvent.keyboard('{Escape}')
    expectStory(document.activeElement === service, 'closing the menu should restore focus to the row')
  },
}

export const StackContextMenuPartial: Story = {
  render: render('desktop'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('.detailRouteStackLink')))
    const stack = Array.from(canvasElement.querySelectorAll<HTMLAnchorElement>('.detailRouteStackLink'))
      .find((row) => row.textContent?.includes('prod'))
    expectStory(Boolean(stack), 'mixed-state stack row should exist')
    fireEvent.contextMenu(stack!)
    const body = within(document.body)
    await waitForCondition(() => Boolean(body.queryByRole('menu')))
    const labels = body.getAllByRole('menuitem').map((item) => item.textContent?.trim())
    expectStory(labels.join('|') === '重启仅部分副本正在运行|停止仅部分副本正在运行|更新', 'partial stack menu should keep lifecycle actions before the separated update action')
    expectStory(Boolean(body.getByText('重启').closest('[data-disabled]')), 'partial stack restart should show its unavailable reason')
    expectStory(Boolean(body.getByText('停止').closest('[data-disabled]')), 'partial stack stop should show its unavailable reason')
    await userEvent.click(body.getByText('更新'))
    await waitForCondition(() => Boolean(globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest))
    const update = globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest as Record<string, unknown>
    expectStory(update.scope === 'stack', 'stack context update should remain stack-scoped')
    expectStory(update.backupMode === 'inherit', 'stack context update should inherit backup policy')
  },
}

export const ServiceContextMenuLongPress: Story = {
  parameters: { viewport: { defaultViewport: 'mobile1' } },
  render: render('mobile'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('.detailRouteServiceLink')))
    const service = canvasElement.querySelector<HTMLAnchorElement>('.detailRouteServiceLink')!
    fireEvent.pointerDown(service, { pointerType: 'touch', pointerId: 1, clientX: 40, clientY: 80 })
    await new Promise((resolve) => setTimeout(resolve, 760))
    const body = within(document.body)
    await waitForCondition(() => Boolean(body.queryByRole('menu')))
    expectStory(Boolean(body.queryByText('重启')), 'touch long press should open the same context menu')
  },
}
