import type { Meta, StoryObj } from '@storybook/react'
import { Popover, PopoverContent, PopoverTrigger } from '../../ui'

function PopoverPreview() {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button className="btn btnGhost" type="button">
          打开摘要
        </button>
      </PopoverTrigger>
      <PopoverContent className="versionTagsPopover" sideOffset={8}>
        <div className="versionTagsPopoverSection" style={{ marginTop: 0 }}>
          <div className="label">发布摘要</div>
          <div className="muted">Popover 可承载轻量说明、快捷操作和悬浮上下文。</div>
        </div>
      </PopoverContent>
    </Popover>
  )
}

const meta: Meta<typeof PopoverPreview> = {
  title: 'Components/Popover',
  component: PopoverPreview,
  tags: ['autodocs'],
}

export default meta

type Story = StoryObj<typeof PopoverPreview>

export const Default: Story = {}
