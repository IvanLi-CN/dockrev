import { useId, useRef, type KeyboardEvent } from 'react'
import type { BackupTargetPolicy } from '../api'

export const BACKUP_POLICY_OPTIONS: Array<{
  value: BackupTargetPolicy
  label: string
}> = [
  { value: 'disabled', label: '不备份' },
  { value: 'stop_related_services', label: '停机备份' },
  { value: 'live_backup', label: '在线备份' },
]

export function backupPolicyIndex(policy: BackupTargetPolicy): number {
  const index = BACKUP_POLICY_OPTIONS.findIndex((option) => option.value === policy)
  return index >= 0 ? index : 0
}

export function BackupPolicySegmentedControl(props: {
  disabled?: boolean
  itemLabel: string
  onChange: (policy: BackupTargetPolicy) => void
  value: BackupTargetPolicy
}) {
  const groupName = useId()
  const buttonRefs = useRef<Array<HTMLButtonElement | null>>([])
  const selectedIndex = backupPolicyIndex(props.value)

  const selectAt = (index: number) => {
    const next = BACKUP_POLICY_OPTIONS[index]
    if (!next || props.disabled) return
    props.onChange(next.value)
    requestAnimationFrame(() => {
      buttonRefs.current[index]?.focus()
    })
  }

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (props.disabled) return
    if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
      event.preventDefault()
      selectAt((selectedIndex + 1) % BACKUP_POLICY_OPTIONS.length)
      return
    }
    if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
      event.preventDefault()
      selectAt((selectedIndex - 1 + BACKUP_POLICY_OPTIONS.length) % BACKUP_POLICY_OPTIONS.length)
      return
    }
    if (event.key === 'Home') {
      event.preventDefault()
      selectAt(0)
      return
    }
    if (event.key === 'End') {
      event.preventDefault()
      selectAt(BACKUP_POLICY_OPTIONS.length - 1)
    }
  }

  return (
    <div
      aria-disabled={props.disabled ? true : undefined}
      aria-label={`${props.itemLabel} 备份策略`}
      className={`serviceBackupPolicyShell${props.disabled ? ' isDisabled' : ''}`}
      onKeyDown={onKeyDown}
      role="radiogroup"
      style={{ ['--backup-policy-index' as string]: String(selectedIndex) }}
    >
      <div className="serviceBackupPolicyThumb" aria-hidden="true" />
      <div className="serviceBackupPolicyGroup">
        {BACKUP_POLICY_OPTIONS.map((option, index) => {
          const active = option.value === props.value
          return (
            <button
              aria-checked={active}
              className={`serviceBackupPolicyBtn${active ? ' active' : ''}`}
              data-state={active ? 'on' : 'off'}
              disabled={props.disabled}
              key={`${groupName}-${option.value}`}
              onClick={() => selectAt(index)}
              ref={(node) => {
                buttonRefs.current[index] = node
              }}
              role="radio"
              tabIndex={active ? 0 : -1}
              type="button"
            >
              {option.label}
            </button>
          )
        })}
      </div>
    </div>
  )
}
