import { Download, FolderPlus, Share2, X } from 'lucide-react'
import { useState } from 'react'

import { usePwaStatus } from '../pwaStatus'
import { IconButton, Popover, PopoverContent, PopoverTrigger } from '../ui'

type PwaInstallControlPlacement = 'sidebar' | 'mobile'

function guideCopy(platform: 'ios' | 'macos') {
  if (platform === 'ios') {
    return {
      title: '添加 Dockrev 到主屏幕',
      steps: [
        { icon: Share2, text: '点击浏览器中的分享按钮。' },
        { icon: Download, text: '选择“添加到主屏幕”，然后确认添加。' },
      ],
    }
  }

  return {
    title: '将 Dockrev 添加到 Dock',
    steps: [
      { icon: FolderPlus, text: '在 Safari 菜单栏打开“文件”。' },
      { icon: Download, text: '选择“添加到 Dock”，然后确认添加。' },
    ],
  }
}

export function PwaInstallControl(props: { placement?: PwaInstallControlPlacement }) {
  const { installCapability, requestPwaInstall } = usePwaStatus()
  const [guideOpen, setGuideOpen] = useState(false)
  const [requesting, setRequesting] = useState(false)
  const placement = props.placement ?? 'sidebar'

  if (!installCapability) return null

  const button = (
    <IconButton
      className="pwaInstallControlButton"
      disabled={requesting}
      hint="安装 Dockrev"
      title="安装 Dockrev"
      withTooltip={installCapability.kind === 'browser-prompt'}
      onClick={
        installCapability.kind === 'browser-prompt'
          ? () => {
              setRequesting(true)
              void requestPwaInstall()
                .catch((error) => {
                  console.warn('[dockrev] PWA install prompt failed', error)
                })
                .finally(() => setRequesting(false))
            }
          : undefined
      }
    >
      <Download size={16} strokeWidth={2.1} aria-hidden="true" />
    </IconButton>
  )

  if (installCapability.kind === 'browser-prompt') return button

  const copy = guideCopy(installCapability.platform)
  return (
    <Popover open={guideOpen} onOpenChange={setGuideOpen}>
      <PopoverTrigger asChild>{button}</PopoverTrigger>
      <PopoverContent
        align="end"
        aria-label="安装 Dockrev"
        className={`pwaInstallGuide pwaInstallGuide-${placement}`}
        side="top"
        sideOffset={10}
      >
        <div className="pwaInstallGuideHeader">
          <div>
            <strong>{copy.title}</strong>
            <span>完成以下步骤后即可从设备上快速打开。</span>
          </div>
          <IconButton
            title="关闭安装说明"
            className="pwaInstallGuideClose"
            onClick={() => setGuideOpen(false)}
          >
            <X size={15} strokeWidth={2.2} aria-hidden="true" />
          </IconButton>
        </div>
        <ol className="pwaInstallGuideSteps">
          {copy.steps.map((step, index) => {
            const StepIcon = step.icon
            return (
              <li key={step.text}>
                <span className="pwaInstallGuideStepNumber">{index + 1}</span>
                <StepIcon size={16} strokeWidth={2.1} aria-hidden="true" />
                <span>{step.text}</span>
              </li>
            )
          })}
        </ol>
      </PopoverContent>
    </Popover>
  )
}
