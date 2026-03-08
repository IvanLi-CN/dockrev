import { useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { ToggleGroup, ToggleGroupItem } from '../../ui'

function ToggleGroupPreview() {
  const [value, setValue] = useState('15m')
  return (
    <ToggleGroup aria-label="时间窗口" className="svcResourceWindowSwitch" type="single" value={value} onValueChange={(next) => next && setValue(next)}>
      <ToggleGroupItem className={value === '15m' ? 'svcResourceWindowBtn active' : 'svcResourceWindowBtn'} value="15m">
        15m
      </ToggleGroupItem>
      <ToggleGroupItem className={value === '1h' ? 'svcResourceWindowBtn active' : 'svcResourceWindowBtn'} value="1h">
        1h
      </ToggleGroupItem>
      <ToggleGroupItem className={value === '6h' ? 'svcResourceWindowBtn active' : 'svcResourceWindowBtn'} value="6h">
        6h
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
