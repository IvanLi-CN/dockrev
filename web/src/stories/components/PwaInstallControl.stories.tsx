import type { Meta, StoryObj } from '@storybook/react'
import type { ReactNode } from 'react'
import { expect, userEvent, within, fn } from 'storybook/test'

import { PwaInstallControl } from '../../components/PwaInstallControl'

function StorySurface(props: { children: ReactNode }) {
  return (
    <div className="pwaInstallStorySurface" data-visual-evidence-surface>
      <div data-visual-evidence-target>{props.children}</div>
    </div>
  )
}

const meta: Meta<typeof PwaInstallControl> = {
  title: 'Components/PwaInstallControl',
  component: PwaInstallControl,
  parameters: {
    layout: 'fullscreen',
  },
}

export default meta

type Story = StoryObj<typeof PwaInstallControl>

export const BrowserPrompt: Story = {
  parameters: {
    pwaStatus: {
      installCapability: { kind: 'browser-prompt' },
      requestPwaInstall: fn(async () => {}),
    },
  },
  render: () => (
    <StorySurface>
      <PwaInstallControl />
    </StorySurface>
  ),
  play: async ({ canvasElement }) => {
    const button = within(canvasElement).getByRole('button', { name: '安装 Dockrev' })
    await userEvent.click(button)
    expect(button).toBeInTheDocument()
  },
}

export const SafariIosGuide: Story = {
  parameters: {
    pwaStatus: {
      installCapability: { kind: 'safari-guide', platform: 'ios' },
    },
  },
  render: () => (
    <StorySurface>
      <PwaInstallControl />
    </StorySurface>
  ),
  play: async ({ canvasElement }) => {
    const button = within(canvasElement).getByRole('button', { name: '安装 Dockrev' })
    await userEvent.click(button)
    const dialog = within(canvasElement.ownerDocument.body).getByRole('dialog', { name: '安装 Dockrev' })
    expect(dialog).toHaveTextContent('添加 Dockrev 到主屏幕')
    expect(dialog).toHaveTextContent('添加到主屏幕')
  },
}

export const SafariMacGuide: Story = {
  parameters: {
    pwaStatus: {
      installCapability: { kind: 'safari-guide', platform: 'macos' },
    },
  },
  render: () => (
    <StorySurface>
      <PwaInstallControl />
    </StorySurface>
  ),
}

export const Unavailable: Story = {
  parameters: {
    pwaStatus: {
      installCapability: null,
    },
  },
  render: () => (
    <StorySurface>
      <PwaInstallControl />
    </StorySurface>
  ),
  play: async ({ canvasElement }) => {
    expect(within(canvasElement).queryByRole('button', { name: '安装 Dockrev' })).not.toBeInTheDocument()
  },
}
