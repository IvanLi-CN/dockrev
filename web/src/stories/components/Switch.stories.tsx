import { useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { Switch } from '../../ui'

const meta: Meta<typeof Switch> = {
  title: 'Components/Switch',
  component: Switch,
  tags: ['autodocs'],
}

export default meta

type Story = StoryObj<typeof Switch>

export const Off: Story = {
  render: () => <Switch checked={false} onChange={() => {}} />,
}

export const On: Story = {
  render: () => <Switch checked onChange={() => {}} />,
}

export const Disabled: Story = {
  render: () => <Switch checked disabled onChange={() => {}} />,
}

function SwitchExample() {
  const [checked, setChecked] = useState(false)
  return <Switch checked={checked} onChange={setChecked} />
}

export const Interactive: Story = {
  render: () => <SwitchExample />,
}
