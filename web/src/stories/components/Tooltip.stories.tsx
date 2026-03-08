import type { Meta, StoryObj } from '@storybook/react'
import { Tooltip, TooltipContent, TooltipTrigger } from '../../ui'

function TooltipPreview() {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button className="btn btnGhost" type="button">
          Hover me
        </button>
      </TooltipTrigger>
      <TooltipContent>显示补充说明</TooltipContent>
    </Tooltip>
  )
}

const meta: Meta<typeof TooltipPreview> = {
  title: 'Components/Tooltip',
  component: TooltipPreview,
  tags: ['autodocs'],
}

export default meta

type Story = StoryObj<typeof TooltipPreview>

export const Default: Story = {}
