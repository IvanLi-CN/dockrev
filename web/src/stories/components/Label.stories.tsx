import type { Meta, StoryObj } from '@storybook/react'
import { Input, Label } from '../../ui'

function LabelPreview() {
  return (
    <div style={{ display: 'grid', gap: 8, maxWidth: 320 }}>
      <Label htmlFor="label-preview-input">Service ID</Label>
      <Input id="label-preview-input" className="input" defaultValue="svc-api" />
    </div>
  )
}

const meta: Meta<typeof Label> = {
  title: 'Components/Label',
  component: Label,
  tags: ['autodocs'],
  render: () => <LabelPreview />,
}

export default meta

type Story = StoryObj<typeof Label>

export const Default: Story = {}
