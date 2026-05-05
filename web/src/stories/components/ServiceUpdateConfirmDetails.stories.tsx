import type { Meta, StoryObj } from '@storybook/react'

import type { Service } from '../../api'
import { ServiceUpdateConfirmDetails } from '../../components/ServiceUpdateConfirmDetails'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof ServiceUpdateConfirmDetails> = {
  title: 'Components/ServiceUpdateConfirmDetails',
  component: ServiceUpdateConfirmDetails,
  decorators: [withDockrevMockApi],
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof ServiceUpdateConfirmDetails>

const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

function baseService(): Service {
  return {
    id: 'svc-confirm-api',
    name: 'api',
    image: {
      ref: 'ghcr.io/acme/api:latest',
      tag: 'latest',
      digest: d('a', '11'),
      resolvedTag: 'v5.2.1',
      resolvedTags: ['v5.2.1', 'latest'],
    },
    candidate: {
      tag: 'latest',
      digest: d('b', '22'),
      resolvedTag: 'v5.2.3',
      archMatch: 'match',
      arch: ['linux/amd64'],
    },
    ignore: null,
    settings: {
      autoRollback: true,
      backupTargets: { bindPaths: {}, volumeNames: {} },
      repoUrl: 'https://github.com/acme/api',
    },
    archived: false,
    versionInference: { status: 'ready', checkedAt: '2026-05-05T10:00:00+08:00', reason: null },
  }
}

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function parseRgb(value: string): [number, number, number] | null {
  const match = /rgba?\((\d+(?:\.\d+)?),\s*(\d+(?:\.\d+)?),\s*(\d+(?:\.\d+)?)/.exec(value)
  if (!match) return null
  return [Number(match[1]), Number(match[2]), Number(match[3])]
}

function relativeLuminance([r, g, b]: [number, number, number]): number {
  const convert = (value: number) => {
    const channel = value / 255
    return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * convert(r) + 0.7152 * convert(g) + 0.0722 * convert(b)
}

function contrastRatio(foreground: string, background: string): number {
  const fg = parseRgb(foreground)
  const bg = parseRgb(background)
  expectStory(fg, `expected parseable foreground color: ${foreground}`)
  expectStory(bg, `expected parseable background color: ${background}`)

  const fgLuminance = relativeLuminance(fg)
  const bgLuminance = relativeLuminance(bg)
  const lighter = Math.max(fgLuminance, bgLuminance)
  const darker = Math.min(fgLuminance, bgLuminance)
  return (lighter + 0.05) / (darker + 0.05)
}

function assertBadgeContrast(canvasElement: HTMLElement) {
  for (const badge of Array.from(canvasElement.querySelectorAll<HTMLElement>('.confirmSignalBadge'))) {
    const label = badge.querySelector<HTMLElement>('.mono')
    expectStory(label, 'expected badge label for contrast check')
    const badgeStyle = getComputedStyle(badge)
    const labelStyle = getComputedStyle(label)
    expectStory(
      contrastRatio(labelStyle.color, badgeStyle.backgroundColor) >= 4.5,
      `badge contrast should meet WCAG AA for ${label.textContent ?? 'unknown badge'}`,
    )
  }
}

export const FloatingTagResolved: Story = {
  args: {
    service: baseService(),
    status: 'updatable',
  },
  render: (args) => (
    <div className="card" style={{ width: 760 }}>
      <ServiceUpdateConfirmDetails {...args} />
    </div>
  ),
  play: async ({ canvasElement }) => {
    const text = canvasElement.textContent ?? ''
    expectStory(text.includes('版本'), 'single-service confirm should show version label')
    expectStory(text.includes('目标 digest'), 'single-service confirm should show target digest label')
    expectStory(text.includes('v5.2.1'), 'current resolved version should be visible')
    expectStory(text.includes('v5.2.3'), 'target resolved version should be visible')
    expectStory(!text.includes('版本latest'), 'floating raw tag must not be the only version signal')
    expectStory(
      canvasElement.querySelector('.confirmSignalBadge-action')?.textContent?.includes('updatable'),
      'status should render as an action badge',
    )
    expectStory(
      canvasElement.querySelector('.confirmSignalBadge-guard')?.textContent?.includes('disallow'),
      'arch policy should render as a guard badge',
    )
    assertBadgeContrast(canvasElement)
  },
}

export const SameTagDigestOnly: Story = {
  render: () => {
    const service = {
      ...baseService(),
      image: {
        ...baseService().image,
        resolvedTag: 'v5.2.3',
        resolvedTags: ['v5.2.3', 'latest'],
      },
      candidate: {
        ...baseService().candidate!,
        resolvedTag: 'v5.2.3',
      },
    } satisfies Service

    return (
      <div className="card" style={{ width: 760 }}>
        <ServiceUpdateConfirmDetails service={service} status="updatable" />
      </div>
    )
  },
  play: async ({ canvasElement }) => {
    const text = canvasElement.textContent ?? ''
    expectStory(text.includes('版本'), 'dialog should show version summary')
    expectStory(text.includes('同标签新 digest'), 'same display update should show digest-only hint')
  },
}

export const HintStatusBadge: Story = {
  render: () => {
    const service = {
      ...baseService(),
      candidate: {
        ...baseService().candidate!,
        archMatch: 'unknown',
        arch: ['linux/amd64', 'linux/arm64'],
      },
    } satisfies Service

    return (
      <div className="card" style={{ width: 760 }}>
        <ServiceUpdateConfirmDetails service={service} status="hint" />
      </div>
    )
  },
  play: async ({ canvasElement }) => {
    expectStory(
      canvasElement.querySelector('.confirmSignalBadge-warn')?.textContent?.includes('hint'),
      'hint status should render as a warning badge',
    )
    expectStory(
      !canvasElement.querySelector('.confirmSignalBadge-action')?.textContent?.includes('hint'),
      'hint status must not reuse the action badge tone',
    )
    assertBadgeContrast(canvasElement)
  },
}

export const LongStatusNarrow: Story = {
  render: () => (
    <div className="card" style={{ width: 320 }}>
      <ServiceUpdateConfirmDetails
        service={baseService()}
        status="blocked-by-maintenance-window-and-requires-manual-operator-review"
      />
    </div>
  ),
  play: async ({ canvasElement }) => {
    const card = canvasElement.querySelector<HTMLElement>('.card')
    const badge = canvasElement.querySelector<HTMLElement>('.confirmSignalBadge-neutral')
    expectStory(card, 'expected narrow card to render')
    expectStory(badge, 'long unknown status should render as a neutral badge')

    const cardBounds = card.getBoundingClientRect()
    const badgeBounds = badge.getBoundingClientRect()
    expectStory(
      badgeBounds.right <= cardBounds.right + 1,
      'long status badge should stay inside its container',
    )
    assertBadgeContrast(canvasElement)
  },
}

export const BadgeContrastLight: Story = {
  globals: {
    backgrounds: { value: 'light' },
    theme: 'light',
  },
  render: () => (
    <div className="card" style={{ width: 760 }}>
      <ServiceUpdateConfirmDetails service={baseService()} status="updatable" />
    </div>
  ),
  play: async ({ canvasElement }) => {
    expectStory(
      document.documentElement.dataset.theme === 'light',
      'expected light theme tokens for badge contrast proof',
    )
    assertBadgeContrast(canvasElement)
  },
}
