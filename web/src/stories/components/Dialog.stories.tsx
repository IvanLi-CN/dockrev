import type { Meta, StoryObj } from '@storybook/react'
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '../../ui'

function DialogPreview() {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <button className="btn btnPrimary" type="button">
          打开详情
        </button>
      </DialogTrigger>
      <DialogContent className="modalCard discoveryIssueDialogCard">
        <DialogHeader className="modalHeader">
          <DialogTitle asChild>
            <div className="modalTitle">forward-auth</div>
          </DialogTitle>
          <DialogDescription asChild>
            <div className="modalBody discoveryIssueDialogBody">
              <div className="discoveryIssueDialogSummary">发现扫描已标记告警，请检查 compose 与挂载状态。</div>
              <div className="discoveryIssueDialogMeta">
                <span className="discoveryIssueDialogMetaItem">最近发现 04/05 16:46</span>
                <span className="discoveryIssueDialogMetaItem">配置 docker-compose.yml +2</span>
                <span className="discoveryIssueDialogMetaItem">关联 stack-prod</span>
              </div>
              <div className="discoveryIssueDialogSectionLabel">完整异常详情</div>
              <pre className="discoveryIssueDialogError">
                {`no canonical superset found; all extra files unreadable; using common compose files.\nHint: mount the override path into dockrev and set DOCKREV_SUPERVISOR_STATE_PATH consistently.`}
              </pre>
            </div>
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="modalActions">
          <DialogClose asChild>
            <button type="button" className="btn btnGhost">
              关闭
            </button>
          </DialogClose>
          <button type="button" className="btn btnGhost">
            复制完整详情
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

const meta: Meta<typeof DialogPreview> = {
  title: 'Components/Dialog',
  component: DialogPreview,
  tags: ['autodocs'],
}

export default meta

type Story = StoryObj<typeof DialogPreview>

export const Default: Story = {}
