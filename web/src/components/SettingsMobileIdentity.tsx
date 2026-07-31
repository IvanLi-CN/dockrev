import { ShieldCheck, UserRound } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import type { TopbarAuthIdentity } from '../topbarAuthIdentity'

export function SettingsMobileIdentity(props: { authIdentity: TopbarAuthIdentity }) {
  const { authIdentity } = props
  const displayName = authIdentity.currentUser === '-' ? authIdentity.triggerLabel : authIdentity.currentUser

  return (
    <section className="settingsMobileIdentity" aria-labelledby="settingsMobileIdentityTitle">
      <Avatar className="settingsMobileIdentityAvatar">
        {authIdentity.avatarUrl ? (
          <AvatarImage src={authIdentity.avatarUrl} alt="" />
        ) : null}
        <AvatarFallback className="settingsMobileIdentityFallback">
          <UserRound size={20} strokeWidth={2.1} aria-hidden="true" />
        </AvatarFallback>
      </Avatar>

      <div className="settingsMobileIdentityBody">
        <div className="settingsMobileIdentityHeading">
          <div id="settingsMobileIdentityTitle" className="settingsMobileIdentityName">
            {displayName}
          </div>
          <Badge className="settingsMobileIdentityBadge" variant="outline">
            <ShieldCheck size={12} strokeWidth={2.2} aria-hidden="true" />
            {authIdentity.authSource}
          </Badge>
        </div>
        <div className="settingsMobileIdentityMeta">
          {authIdentity.currentGroups === '-' ? '未识别用户组' : authIdentity.currentGroups}
        </div>
      </div>
    </section>
  )
}
