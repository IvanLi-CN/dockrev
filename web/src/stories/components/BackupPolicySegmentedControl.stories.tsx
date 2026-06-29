import { useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import type { BackupTargetPolicy } from '../../api'
import { BackupPolicySegmentedControl } from '../../components/BackupPolicySegmentedControl'

function BackupPolicySegmentedControlPreview(props: {
  disabled?: boolean
  initialValue?: BackupTargetPolicy
  itemLabel?: string
}) {
  const [value, setValue] = useState<BackupTargetPolicy>(props.initialValue ?? 'stop_related_services')
  return (
    <div style={{ width: 'min(100%, 520px)' }}>
      <BackupPolicySegmentedControl
        disabled={props.disabled}
        itemLabel={props.itemLabel ?? 'api-cache'}
        onChange={setValue}
        value={value}
      />
    </div>
  )
}

const meta: Meta<typeof BackupPolicySegmentedControlPreview> = {
  title: 'Components/BackupPolicySegmentedControl',
  component: BackupPolicySegmentedControlPreview,
  tags: ['autodocs'],
  args: {
    disabled: false,
    initialValue: 'stop_related_services',
    itemLabel: 'api-cache',
  },
}

export default meta

type Story = StoryObj<typeof BackupPolicySegmentedControlPreview>

export const StopServices: Story = {}

export const LiveBackup: Story = {
  args: {
    initialValue: 'live_backup',
  },
}

export const DisabledPolicy: Story = {
  args: {
    initialValue: 'disabled',
  },
}

export const ReadOnly: Story = {
  args: {
    disabled: true,
  },
}
