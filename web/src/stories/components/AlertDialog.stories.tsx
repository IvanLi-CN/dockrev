import type { Meta, StoryObj } from '@storybook/react'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '../../components/ui/alert-dialog'

function AlertDialogPreview() {
  return (
    <AlertDialog>
      <AlertDialogTrigger asChild>
        <button className="btn btnDanger" type="button">
          打开确认
        </button>
      </AlertDialogTrigger>
      <AlertDialogContent className="modalCard">
        <AlertDialogHeader className="modalHeader">
          <AlertDialogTitle asChild>
            <div className="modalTitle">确认删除 webhook？</div>
          </AlertDialogTitle>
          <AlertDialogDescription asChild>
            <div className="modalBody">AlertDialog 适合高风险确认、不可逆操作与离开页面前提示。</div>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter className="modalActions">
          <AlertDialogCancel className="btn btnGhost">取消</AlertDialogCancel>
          <AlertDialogAction className="btn btnDanger">删除</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

const meta: Meta<typeof AlertDialogPreview> = {
  title: 'Components/AlertDialog',
  component: AlertDialogPreview,
  tags: ['autodocs'],
}

export default meta

type Story = StoryObj<typeof AlertDialogPreview>

export const Default: Story = {}
