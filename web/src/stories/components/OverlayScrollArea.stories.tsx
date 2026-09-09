import type { Meta, StoryObj } from '@storybook/react'
import { OverlayScrollArea } from '../../ui'

function ScrollAreaPreview(props: { horizontal?: boolean }) {
  const entries = Array.from({ length: props.horizontal ? 12 : 24 }, (_, index) => `操作记录 ${String(index + 1).padStart(2, '0')}`)
  return (
    <div className="overlayScrollAreaStorySurface" data-visual-evidence-surface>
      <div className="overlayScrollAreaStoryTarget" data-visual-evidence-target>
        <OverlayScrollArea
          className={props.horizontal ? 'overlayScrollAreaStory overlayScrollAreaStoryHorizontal' : 'overlayScrollAreaStory'}
          options={{ overflow: props.horizontal ? { x: 'scroll', y: 'hidden' } : { x: 'hidden', y: 'scroll' } }}
          viewportLabel={props.horizontal ? '横向操作记录' : '纵向操作记录'}
        >
          <div className={props.horizontal ? 'overlayScrollAreaStoryRow' : 'overlayScrollAreaStoryList'}>
            {entries.map((entry) => (
              <div className="overlayScrollAreaStoryItem" key={entry}>
                {entry}
              </div>
            ))}
          </div>
        </OverlayScrollArea>
      </div>
    </div>
  )
}

function NoOverflowPreview() {
  return (
    <div className="overlayScrollAreaStorySurface" data-visual-evidence-surface>
      <div className="overlayScrollAreaStoryTarget" data-visual-evidence-target>
        <OverlayScrollArea className="overlayScrollAreaStory overlayScrollAreaStoryNoOverflow" viewportLabel="无溢出内容">
          <div className="overlayScrollAreaStoryList">
            <div className="overlayScrollAreaStoryItem">当前内容无需滚动</div>
          </div>
        </OverlayScrollArea>
      </div>
    </div>
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
    expectStory(viewport.getAttribute('role') === 'region', 'Viewport should expose a named region')
    expectStory(viewport.tabIndex === 0, 'Viewport should be keyboard focusable')
    expectStory(canvasElement.querySelector('.os-scrollbar-auto-hide'), 'Scrollbar should opt into auto-hide')
    viewport.focus()
    expectStory(viewport.ownerDocument.activeElement === viewport, 'Viewport should accept keyboard focus')
  },
}

export const Horizontal: Story = {
  args: {
    horizontal: true,
  },
}

export const NoOverflow: Story = {
  render: () => <NoOverflowPreview />,
  play: async ({ canvasElement }) => {
    await new Promise((resolve) => setTimeout(resolve, 250))
    const viewport = canvasElement.querySelector<HTMLElement>('[data-overlayscrollbars-viewport]')
    expectStory(viewport, 'No-overflow story should create a viewport')
    expectStory(viewport.scrollHeight <= viewport.clientHeight, 'No-overflow story should fit without vertical scrolling')
    expectStory(
      Array.from(canvasElement.querySelectorAll('.os-scrollbar')).every((scrollbar) => scrollbar.classList.contains('os-scrollbar-unusable')),
      'No-overflow story should keep scrollbar tracks unusable',
    )
  },
}
