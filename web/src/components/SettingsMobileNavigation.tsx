import {
  Activity,
  ArchiveRestore,
  Bell,
  BookOpenText,
  ChevronLeft,
  ChevronRight,
  Clock3,
  Plug,
  UserRound,
  Wrench,
  type LucideIcon,
} from 'lucide-react'
import { navigate, type SettingsSection } from '../routes'
import { Button } from '../ui'

export type SettingsMobileDestination = {
  section: SettingsSection
  title: string
  description: string
  icon: LucideIcon
}

export const SETTINGS_DESTINATIONS: SettingsMobileDestination[] = [
    { section: 'account', title: '账户与鉴权', description: '当前身份与 Forward Auth', icon: UserRound },
  { section: 'maintenance', title: '维护工具', description: '自我升级与部署检查', icon: Wrench },
  { section: 'backup', title: '备份', description: '更新前备份默认策略', icon: ArchiveRestore },
  { section: 'monitoring', title: '资源监控', description: '采样频率与历史保留', icon: Activity },
  { section: 'schedules', title: '定时任务', description: '更新检查与 Webhook 巡查', icon: Clock3 },
  { section: 'release-notes', title: '更新日志', description: 'Release notes 数据源', icon: BookOpenText },
  { section: 'notifications', title: '通知', description: '事件与发送渠道', icon: Bell },
  { section: 'integrations', title: '实例与集成', description: 'Public URL 与 GitHub Packages', icon: Plug },
]

export function SettingsMobileNavigation(props: { section?: SettingsSection }) {
  const current = SETTINGS_DESTINATIONS.find((item) => item.section === props.section)

  if (current) {
    return (
      <div className="settingsMobileSubpageHeader">
        <Button
          className="settingsMobileBackButton"
          variant="ghost"
          ariaLabel="返回系统设置"
          title="返回系统设置"
          onClick={() => navigate({ name: 'settings' })}
        >
          <ChevronLeft size={18} strokeWidth={2.2} aria-hidden="true" />
        </Button>
        <div className="settingsMobileSubpageTitle">{current.title}</div>
      </div>
    )
  }

  return (
    <nav className="settingsMobileIndex" aria-label="设置分类">
      {SETTINGS_DESTINATIONS.map((item) => {
        const ItemIcon = item.icon
        return (
          <Button
            key={item.section}
            className="settingsMobileIndexItem"
            variant="ghost"
            onClick={() => navigate({ name: 'settings', section: item.section })}
          >
            <span className="settingsMobileIndexIcon" aria-hidden="true">
              <ItemIcon size={18} strokeWidth={2.1} />
            </span>
            <span className="settingsMobileIndexCopy">
              <span className="settingsMobileIndexTitle">{item.title}</span>
              <span className="settingsMobileIndexDescription">{item.description}</span>
            </span>
            <ChevronRight className="settingsMobileIndexChevron" size={17} strokeWidth={2.1} aria-hidden="true" />
          </Button>
        )
      })}
    </nav>
  )
}
