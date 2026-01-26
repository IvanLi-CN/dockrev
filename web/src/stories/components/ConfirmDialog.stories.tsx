import type { Meta, StoryObj } from '@storybook/react'
import { useState } from 'react'
import { ConfirmProvider } from '../../ConfirmProvider'
import { useConfirm } from '../../confirm'
import { Mono } from '../../ui'

function ConfirmSandbox() {
  const confirm = useConfirm()
  const [last, setLast] = useState<string>('—')

  return (
    <div style={{ padding: 16, display: 'grid', gap: 12, maxWidth: 720 }}>
      <div className="muted">
        last result: <span className="mono">{last}</span>
      </div>

      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
        <button
          className="btn btnPrimary"
          onClick={() => {
            void (async () => {
              const ok = await confirm({
                title: '确认执行发现扫描？',
                body: (
                  <>
                    <div className="modalLead">发现扫描会拉取 discovery projects，并标记 missing/invalid。</div>
                    <div className="modalKvGrid">
                      <div className="modalKvLabel">操作</div>
                      <div className="modalKvValue">
                        <Mono>discovery scan</Mono>
                      </div>
                      <div className="modalKvLabel">可能影响</div>
                      <div className="modalKvValue">创建/更新 stacks，或将 stacks 标记为 missing/invalid。</div>
                    </div>
                  </>
                ),
                confirmText: '开始扫描',
                cancelText: '取消',
                confirmVariant: 'primary',
                badgeText: '扫描任务',
                badgeTone: 'warn',
              })
              setLast(ok ? 'ok' : 'cancel')
            })()
          }}
        >
          打开：发现扫描
        </button>

        <button
          className="btn btnPrimary"
          onClick={() => {
            void (async () => {
              const ok = await confirm({
                title: '确认更新服务 svc-api？',
                body: (
                  <>
                    <div className="modalLead">将拉取镜像并重启容器；失败可能触发回滚。</div>
                    <div className="modalKvGrid">
                      <div className="modalKvLabel">范围</div>
                      <div className="modalKvValue">
                        <Mono>service</Mono>
                      </div>
                      <div className="modalKvLabel">目标</div>
                      <div className="modalKvValue">
                        <Mono>stack-prod/svc-api</Mono>
                      </div>
                      <div className="modalKvLabel">当前 → 候选</div>
                      <div className="modalKvValue">
                        <span className="mono" title="v1.2.3@sha256:... → v1.3.0@sha256:...">
                          v1.2.3 → v1.3.0
                        </span>
                      </div>
                    </div>
                  </>
                ),
                confirmText: '执行更新',
                cancelText: '取消',
                confirmVariant: 'primary',
                badgeText: '将更新并重启',
                badgeTone: 'warn',
              })
              setLast(ok ? 'ok' : 'cancel')
            })()
          }}
        >
          打开：服务更新
        </button>

        <button
          className="btn btnDanger"
          onClick={() => {
            void (async () => {
              const ok = await confirm({
                title: '确认执行更新？',
                body: (
                  <>
                    <div className="modalLead">将拉取镜像并重启容器；失败可能触发回滚。</div>
                    <div className="modalKvGrid">
                      <div className="modalKvLabel">范围</div>
                      <div className="modalKvValue">
                        <Mono>stack</Mono>
                      </div>
                      <div className="modalKvLabel">目标</div>
                      <div className="modalKvValue">
                        <Mono>stack-prod</Mono>
                      </div>
                    </div>
                  </>
                ),
                confirmText: '执行更新',
                cancelText: '取消',
                confirmVariant: 'danger',
                badgeText: '批量更新',
                badgeTone: 'bad',
              })
              setLast(ok ? 'ok' : 'cancel')
            })()
          }}
        >
          打开：堆栈更新
        </button>

        <button
          className="btn btnDanger"
          onClick={() => {
            void (async () => {
              const ok = await confirm({
                title: '确认执行更新？',
                body: (
                  <>
                    <div className="modalLead">将拉取镜像并重启容器；失败可能触发回滚。</div>
                    <div className="modalKvGrid">
                      <div className="modalKvLabel">范围</div>
                      <div className="modalKvValue">
                        <Mono>all</Mono>
                      </div>
                      <div className="modalKvLabel">目标</div>
                      <div className="modalKvValue">
                        <Mono>all stacks</Mono>
                      </div>
                    </div>
                  </>
                ),
                confirmText: '执行更新',
                cancelText: '取消',
                confirmVariant: 'danger',
                badgeText: '全量更新',
                badgeTone: 'bad',
              })
              setLast(ok ? 'ok' : 'cancel')
            })()
          }}
        >
          打开：全量更新
        </button>
      </div>
    </div>
  )
}

function WithProvider() {
  return (
    <ConfirmProvider>
      <ConfirmSandbox />
    </ConfirmProvider>
  )
}

const meta: Meta<typeof WithProvider> = {
  title: 'Components/ConfirmDialog',
  component: WithProvider,
}

export default meta
type Story = StoryObj<typeof WithProvider>

export const Demo: Story = {}

