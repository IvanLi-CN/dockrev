import type { AutoUpdatePolicy } from '../api'
import { ResponsiveSettingsDrawer } from './ResponsiveSettingsDrawer'
import { AutoUpdatePolicyEditor } from './AutoUpdatePolicyEditor'

export function AutoUpdatePolicyDrawer(props: {
  busy?: boolean
  onChange: (policy: AutoUpdatePolicy) => void
  onOpenChange: (open: boolean) => void
  onSave: () => void
  open: boolean
  policy: AutoUpdatePolicy
  previewServiceId?: string
  scope: 'service' | 'stack'
  stackPolicy?: AutoUpdatePolicy | null
}) {
  return (
    <ResponsiveSettingsDrawer
      description="配置候选版本匹配规则、自动部署动作和延迟门槛。"
      onOpenChange={props.onOpenChange}
      open={props.open}
      title="自动更新策略"
    >
      <AutoUpdatePolicyEditor
        busy={props.busy}
        onChange={props.onChange}
        onSave={props.onSave}
        policy={props.policy}
        previewServiceId={props.previewServiceId}
        scope={props.scope}
        stackPolicy={props.stackPolicy}
      />
    </ResponsiveSettingsDrawer>
  )
}
