import { useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../../ui'

function SelectPreview(props: { disabled?: boolean }) {
  const [value, setValue] = useState('ready')
  return (
    <div style={{ maxWidth: 280 }}>
      <Select disabled={props.disabled} value={value} onValueChange={setValue}>
        <SelectTrigger className="select">
          <SelectValue placeholder="选择状态" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="ready">Ready</SelectItem>
          <SelectItem value="queued">Queued</SelectItem>
          <SelectItem value="failed">Failed</SelectItem>
        </SelectContent>
      </Select>
    </div>
  )
}

const meta: Meta<typeof SelectPreview> = {
  title: 'Components/Select',
  component: SelectPreview,
  tags: ['autodocs'],
  args: {
    disabled: false,
  },
}

export default meta

type Story = StoryObj<typeof SelectPreview>

export const Default: Story = {}
export const Disabled: Story = { args: { disabled: true } }
