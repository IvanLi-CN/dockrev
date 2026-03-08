import { useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { SelectField } from '../../ui'

function SelectFieldPreview(props: { disabled?: boolean }) {
  const [value, setValue] = useState('all')
  return (
    <div style={{ maxWidth: 280 }}>
      <SelectField
        className="select"
        disabled={props.disabled}
        onChange={setValue}
        options={[
          { value: 'all', label: '全部' },
          { value: 'ready', label: 'Ready' },
          { value: 'failed', label: 'Failed' },
        ]}
        value={value}
      />
    </div>
  )
}

const meta: Meta<typeof SelectFieldPreview> = {
  title: 'Components/SelectField',
  component: SelectFieldPreview,
  tags: ['autodocs'],
  args: {
    disabled: false,
  },
}

export default meta

type Story = StoryObj<typeof SelectFieldPreview>

export const Default: Story = {}
export const Disabled: Story = { args: { disabled: true } }
