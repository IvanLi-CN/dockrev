import type { Meta, StoryObj } from '@storybook/react'
import { useState } from 'react'
import { ConfirmProvider } from '../../ConfirmProvider'
import { useConfirm } from '../../confirm'
import { Mono } from '../../ui'
import { UpdateTargetSelect } from '../../components/UpdateTargetSelect'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

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
              const selected = { tag: '5.2.4', digest: null as string | null }
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
                      <div className="modalKvLabel">当前 → 目标</div>
                      <div className="modalKvValue">
                        <span className="mono">5.2.1</span>
                        <span className="mono" style={{ opacity: 0.8 }}>
                          {' '}
                          →{' '}
                        </span>
                        <UpdateTargetSelect
                          serviceId="svc-prod-api"
                          currentTag="5.2.1"
                          initialTag="5.2.4"
                          initialDigest={null}
                          variant="inline"
                          showLabel={false}
                          showComparison={false}
                          onChange={(next) => {
                            selected.tag = next.tag
                            selected.digest = next.digest ?? null
                          }}
                        />
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
              setLast(ok ? `ok (target=${selected.tag})` : 'cancel')
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
                    <div className="modalLead">将为该 stack 内服务创建更新任务（服务端会计算是否实际变更）。</div>
                    <div className="modalKvGrid">
                      <div className="modalKvLabel">范围</div>
                      <div className="modalKvValue">
                        <Mono>stack</Mono>
                      </div>
                      <div className="modalKvLabel">目标</div>
                      <div className="modalKvValue">
                        <Mono>stack-prod</Mono>
                      </div>
                      <div className="modalKvLabel">候选服务</div>
                      <div className="modalKvValue">3 个（可更新/需确认/跨标签）</div>
                      <div className="modalKvLabel">其中</div>
                      <div className="modalKvValue">可更新 2 · 需确认 1 · 跨标签 0</div>
                      <div className="modalKvLabel">将跳过</div>
                      <div className="modalKvValue">架构不匹配 0 · 被阻止 1</div>
                    </div>
                    <div className="modalDivider" />
                    <div className="modalLead">将更新的服务（预览）</div>
                    <div className="modalList">
                      <div className="modalListItem">
                        <div className="modalListLeft">
                          <div className="modalListTitle">
                            <span className="mono">svc-api</span>
                            <span className="muted"> · updatable</span>
                          </div>
                          <div className="muted">
                            <span className="mono">ghcr.io/acme/app</span>
                          </div>
                        </div>
                        <div className="modalListRight">
                          <span className="mono" title="v1.2.3@sha256:... → v1.3.0@sha256:...">
                            v1.2.3 → v1.3.0
                          </span>
                        </div>
                      </div>
                      <div className="modalListItem">
                        <div className="modalListLeft">
                          <div className="modalListTitle">
                            <span className="mono">svc-web</span>
                            <span className="muted"> · hint</span>
                          </div>
                          <div className="muted">
                            <span className="mono">ghcr.io/acme/web</span>
                          </div>
                        </div>
                        <div className="modalListRight">
                          <span className="mono" title="v2.0.0@sha256:... → v2.0.1@sha256:...">
                            v2.0.0 → v2.0.1
                          </span>
                        </div>
                      </div>
                      <div className="modalListItem">
                        <div className="modalListLeft">
                          <div className="modalListTitle">
                            <span className="mono">svc-worker</span>
                            <span className="muted"> · updatable</span>
                          </div>
                          <div className="muted">
                            <span className="mono">ghcr.io/acme/worker</span>
                          </div>
                        </div>
                        <div className="modalListRight">
                          <span className="mono" title="v0.9.0@sha256:... → v0.10.0@sha256:...">
                            v0.9.0 → v0.10.0
                          </span>
                        </div>
                      </div>
                    </div>
                    <div className="modalDivider" />
                    <div className="muted">提示：将拉取镜像并重启容器；失败可能触发回滚。</div>
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
              const items = Array.from({ length: 12 }, (_, i) => ({
                name: i === 1 ? 'svc-web' : i === 2 ? 'svc-worker' : `svc-${i + 1}`,
                ref: i === 1 ? 'ghcr.io/acme/web' : i === 2 ? 'ghcr.io/acme/worker' : 'ghcr.io/acme/app',
                status: i === 1 ? 'hint' : i === 2 ? 'updatable' : 'updatable',
                current: i === 0 ? 'v1.2.3' : i === 1 ? 'v2.0.0' : `v0.${i}.0`,
                next: i === 0 ? 'v1.3.0' : i === 1 ? 'v2.0.1' : `v0.${i}.1`,
                title: i === 0 ? 'v1.2.3@sha256:... → v1.3.0@sha256:...' : `${`v0.${i}.0`}@sha256:... → ${`v0.${i}.1`}@sha256:...`,
              }))
              const ok = await confirm({
                title: '确认执行更新？',
                body: (
                  <>
                    <div className="modalLead">将为所有服务创建更新任务（服务端会计算是否实际变更）。</div>
                    <div className="modalKvGrid">
                      <div className="modalKvLabel">范围</div>
                      <div className="modalKvValue">
                        <Mono>all</Mono>
                      </div>
                      <div className="modalKvLabel">目标</div>
                      <div className="modalKvValue">
                        <Mono>all stacks</Mono>
                      </div>
                      <div className="modalKvLabel">候选服务</div>
                      <div className="modalKvValue">5 个（可更新/需确认/跨标签）</div>
                      <div className="modalKvLabel">其中</div>
                      <div className="modalKvValue">可更新 3 · 需确认 1 · 跨标签 1</div>
                      <div className="modalKvLabel">将跳过</div>
                      <div className="modalKvValue">架构不匹配 1 · 被阻止 2</div>
                    </div>
                    <div className="modalDivider" />
                    <div className="modalLead">将更新的服务（预览）</div>
                    <div className="modalList">
                      {items.map((it, idx) => (
                        <div key={idx} className="modalListItem">
                          <div className="modalListLeft">
                            <div className="modalListTitle">
                              <span className="mono">{`stack-prod/${it.name}`}</span>
                              <span className="muted">{` · ${it.status}`}</span>
                            </div>
                            <div className="muted">
                              <span className="mono">{it.ref}</span>
                            </div>
                          </div>
                          <div className="modalListRight">
                            <span className="mono" title={it.title}>{`${it.current} → ${it.next}`}</span>
                          </div>
                        </div>
                      ))}
                    </div>
                    <div className="modalDivider" />
                    <div className="muted">提示：将拉取镜像并重启容器；失败可能触发回滚。</div>
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
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof WithProvider>

export const Demo: Story = {}
