import { useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'

import { PwaUpdateBubble } from '../../components/PwaUpdateBubble'
import { PwaStatusMockProvider } from '../../pwaStatus'

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function ReadyDismissibleStory() {
  const [visible, setVisible] = useState(true)
  return (
    <PwaStatusMockProvider
      value={{
        dismissUpdate: () => setVisible(false),
        updatePhase: 'ready',
        updatePromptVisible: visible,
      }}
    >
      <PwaUpdateBubble />
    </PwaStatusMockProvider>
  )
}

const meta: Meta<typeof PwaUpdateBubble> = {
  title: 'Components/PwaUpdateBubble',
  component: PwaUpdateBubble,
  parameters: {
    layout: 'fullscreen',
  },
}

export default meta
type Story = StoryObj<typeof PwaUpdateBubble>

export const Downloading: Story = {
  parameters: {
    pwaStatus: {
      updatePhase: 'downloading',
      updatePromptVisible: true,
    },
  },
  play: async ({ canvasElement }) => {
    const updateButton = canvasElement.querySelector<HTMLButtonElement>('.pwaUpdateBubble .btnPrimary')
    expectStory(updateButton?.disabled, 'Downloading updates should disable immediate activation')
  },
}

export const Ready: Story = {
  render: () => <ReadyDismissibleStory />,
  play: async ({ canvasElement }) => {
    const later = canvasElement.querySelector<HTMLButtonElement>('.pwaUpdateBubble .btnGhost')
    expectStory(later, 'Ready updates should provide a later action')
    later.click()
    await new Promise((resolve) => window.setTimeout(resolve, 0))
    expectStory(!canvasElement.querySelector('.pwaUpdateBubble'), 'Later should hide the prompt without changing update readiness')
  },
}

export const ReadyWhileOffline: Story = {
  parameters: {
    pwaStatus: {
      isOnline: false,
      updatePhase: 'ready',
      updatePromptVisible: true,
    },
  },
  play: async ({ canvasElement }) => {
    const updateButton = canvasElement.querySelector<HTMLButtonElement>('.pwaUpdateBubble .btnPrimary')
    expectStory(!updateButton?.disabled, 'A complete waiting worker should remain activatable while offline')
  },
}

export const Failed: Story = {
  parameters: {
    pwaStatus: {
      updatePhase: 'failed',
      updatePromptVisible: true,
    },
  },
  play: async ({ canvasElement }) => {
    expectStory(canvasElement.textContent?.includes('重新检查'), 'Failed updates should expose a retry action')
  },
}

export const ReadyMobile: Story = {
  parameters: {
    pwaStatus: {
      updatePhase: 'ready',
      updatePromptVisible: true,
    },
    viewport: { defaultViewport: 'mobile1' },
  },
}
