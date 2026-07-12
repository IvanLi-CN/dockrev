import type { Meta, StoryObj } from '@storybook/react'
import { OverlayScrollArea } from '../../ui'

function ScrollAreaPreview(props: { horizontal?: boolean }) {
  const entries = Array.from({ length: props.horizontal ? 12 : 24 }, (_, index) => `操作记录 ${String(index + 1).padStart(2, '0')}`)
  return (
    <OverlayScrollArea
      className={props.horizontal ? 'overlayScrollAreaStory overlayScrollAreaStoryHorizontal' : 'overlayScrollAreaStory'}
      options={{ overflow: props.horizontal ? { x: 'scroll', y: 'hidden' } : { x: 'hidden', y: 'scroll' } }}
    >
      <div className={props.horizontal ? 'overlayScrollAreaStoryRow' : 'overlayScrollAreaStoryList'}>
        {entries.map((entry) => (
          <div className="overlayScrollAreaStoryItem" key={entry}>
            {entry}
          </div>
        ))}
      </div>
    </OverlayScrollArea>
  )
}

const meta: Meta<typeof ScrollAreaPreview> = {
  title: 'Components/OverlayScrollArea',
  component: ScrollAreaPreview,
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof ScrollAreaPreview>

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message)
}

export const Vertical: Story = {
  play: async ({ canvasElement }) => {
    const viewport = canvasElement.querySelector<HTMLElement>('[data-overlayscrollbars-viewport]')
    expectStory(viewport, 'OverlayScrollArea should create a scrollable viewport')
    expectStory(viewport.scrollHeight > viewport.clientHeight, 'Vertical story should overflow')
    expectStory(canvasElement.querySelector('.os-theme-dockrev'), 'Scrollbar should receive the Dockrev theme')
  },
}

export const Horizontal: Story = {
  args: {
    horizontal: true,
  },
}
