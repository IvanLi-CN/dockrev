import { useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { ToggleGroup, ToggleGroupItem } from '../../ui'

function ToggleGroupPreview() {
  const [value, setValue] = useState('3m')
  return (
    <ToggleGroup aria-label="时间窗口" className="svcResourceWindowSwitch" type="single" value={value} onValueChange={(next) => next && setValue(next)}>
      <ToggleGroupItem className={value === '3m' ? 'svcResourceWindowBtn active' : 'svcResourceWindowBtn'} value="3m">
        3m
      </ToggleGroupItem>
      <ToggleGroupItem className={value === '1h' ? 'svcResourceWindowBtn active' : 'svcResourceWindowBtn'} value="1h">
        1h
      </ToggleGroupItem>
      <ToggleGroupItem className={value === '24h' ? 'svcResourceWindowBtn active' : 'svcResourceWindowBtn'} value="24h">
        24h
      </ToggleGroupItem>
    </ToggleGroup>
  )
}

const meta: Meta<typeof ToggleGroupPreview> = {
  title: 'Components/ToggleGroup',
  component: ToggleGroupPreview,
  tags: ['autodocs'],
}

export default meta

type Story = StoryObj<typeof ToggleGroupPreview>

export const Default: Story = {}
