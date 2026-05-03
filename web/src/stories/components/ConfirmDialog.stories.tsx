import type { Meta, StoryObj } from '@storybook/react'
import { useState } from 'react'
import { ConfirmDialog, ConfirmProvider } from '../../ConfirmProvider'
import { useConfirm } from '../../confirm'
import { Mono } from '../../ui'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const LONG_DIGEST = 'sha256:eda3fe8c1c9d782840ded123b7f16936e4abb4d29e13981d132c27877c2f4680'

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

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
                        <span className="mono">5.2.4</span>
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
                      <div className="modalKvValue">3 个（可更新/需确认）</div>
                      <div className="modalKvLabel">其中</div>
                      <div className="modalKvValue">可更新 2 · 需确认 1</div>
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
	                      <div className="modalKvValue">5 个（可更新/需确认）</div>
	                      <div className="modalKvLabel">其中</div>
	                      <div className="modalKvValue">可更新 3 · 需确认 1</div>
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

export const ServiceUpdateLongDigest: Story = {
  render: () => (
    <div data-story-root="confirm-dialog-long-digest">
      <ConfirmDialog
        title="确认更新服务 ani-rss?"
        body={
          <>
            <div className="modalLead">将对该服务执行更新，并保留默认备份策略。</div>
            <div className="modalKvGrid">
              <div className="modalKvLabel">服务</div>
              <div className="modalKvValue">
                <Mono>media/media-ani-rss</Mono>
              </div>
              <div className="modalKvLabel">当前镜像</div>
              <div className="modalKvValue">
                <Mono>docker.io/wushuo894/ani-rss:latest</Mono>
              </div>
              <div className="modalKvLabel">目标版本</div>
              <div className="modalKvValue">
                <Mono>latest</Mono>
              </div>
              <div className="modalKvLabel">目标 digest</div>
              <div className="modalKvValue">
                <Mono>{LONG_DIGEST}</Mono>
              </div>
            </div>
          </>
        }
        confirmText="执行更新"
        cancelText="取消"
        confirmVariant="primary"
        badgeText="将更新并重启"
        badgeTone="warn"
        onClose={() => undefined}
      />
    </div>
  ),
  play: async ({ canvasElement }) => {
    const card = canvasElement.ownerDocument.querySelector<HTMLElement>('.modalCard')
    const digest = Array.from(canvasElement.ownerDocument.querySelectorAll<HTMLElement>('.modalKvValue .mono')).find(
      (node) => node.textContent === LONG_DIGEST,
    )

    expectStory(card, 'expected confirm dialog card to be rendered')
    expectStory(digest, 'expected long digest to be rendered')

    const cardBounds = card.getBoundingClientRect()
    const digestBounds = digest.getBoundingClientRect()
    expectStory(
      digestBounds.right <= cardBounds.right + 1,
      'long digest should stay inside the confirm dialog card',
    )
  },
}
