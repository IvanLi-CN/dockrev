import { useCallback, type Dispatch, type MutableRefObject, type SetStateAction } from 'react'
import { ApiError, previewServiceVersionUpdate, triggerServiceVersionUpdate, type Service } from '../api'
import { useConfirm } from '../confirm'
import { Mono } from '../ui'
import { navigate } from '../routes'
import { conflictingJobId, errorMessage, shortDigest } from './serviceDetailUtils'
import type { ActiveUpdateJob, UpdateActionTargetKey } from '../updateActionTracking'
import type { AsyncDataTrigger } from '../asyncData'

type Input = {
  applyActionKey: UpdateActionTargetKey | null
  applyActiveJob: ActiveUpdateJob | null
  beginSubmitting: (target: UpdateActionTargetKey) => void
  endSubmitting: (target: UpdateActionTargetKey) => void
  confirm: ReturnType<typeof useConfirm>
  pageGeneration: number
  pageGenerationRef: MutableRefObject<number>
  refreshLifecycleStatus: (generation: number) => Promise<unknown>
  requestRefresh: (trigger?: AsyncDataTrigger) => Promise<unknown>
  service: Service | null
  setError: Dispatch<SetStateAction<string | null>>
  setNotice: Dispatch<SetStateAction<{ jobId: string; kind: 'update' | 'rollback' | 'lifecycle' } | null>>
  submittingTokensRef: MutableRefObject<Map<symbol, UpdateActionTargetKey>>
  trackJob: (target: UpdateActionTargetKey, jobId: string, status?: string, targetVersion?: string | null) => void
}

export function useSelectedVersionUpdateAction(input: Input) {
  const {
    applyActionKey, applyActiveJob, beginSubmitting, confirm, endSubmitting,
    pageGeneration, pageGenerationRef, refreshLifecycleStatus, requestRefresh,
    service, setError, setNotice, submittingTokensRef, trackJob,
  } = input

  return useCallback((releaseTag: string) => {
    void (async () => {
      const generation = pageGeneration
      if (generation !== pageGenerationRef.current || !service) return
      if (applyActiveJob) {
        navigate({ name: 'job', jobId: applyActiveJob.jobId })
        return
      }
      let submissionToken: symbol | null = null
      if (applyActionKey) {
        submissionToken = Symbol(applyActionKey)
        submittingTokensRef.current.set(submissionToken, applyActionKey)
        beginSubmitting(applyActionKey)
      }
      try {
        setError(null)
        const preview = await previewServiceVersionUpdate(service.id, releaseTag)
        if (generation !== pageGenerationRef.current) return
        const forced = preview.classification === 'forced'
        const autoUpdateNotice = '启用的自动更新策略仍会按原计划运行；后续合格检查可能再次把服务更新到届时配置标签指向的摘要。'
        const confirmed = await confirm({
          title: `确认${forced ? '强制更新' : '更新'}服务 ${service.name}？`,
          body: (
            <>
              <div className="modalLead">
                {forced
                  ? '当前镜像仓库和配置标签没有该版本的可信历史关联，将在提交时再次解析这个 Release tag。'
                  : '该版本曾由当前配置标签解析到对应镜像摘要，本次将按该摘要部署。'}
              </div>
              <div className="modalKvGrid">
                <div className="modalKvLabel">当前版本</div>
                <div className="modalKvValue"><Mono>{preview.currentVersion}</Mono></div>
                <div className="modalKvLabel">目标版本</div>
                <div className="modalKvValue"><Mono>{preview.releaseTag}</Mono></div>
                <div className="modalKvLabel">目标摘要</div>
                <div className="modalKvValue"><Mono>{shortDigest(preview.targetDigest)}</Mono></div>
                <div className="modalKvLabel">自动更新</div>
                <div className="modalKvValue">{autoUpdateNotice}</div>
              </div>
            </>
          ),
          confirmText: forced ? '继续强制更新' : '更新',
          cancelText: '取消',
          confirmVariant: forced ? 'danger' : 'primary',
          badgeText: null,
        })
        if (!confirmed || generation !== pageGenerationRef.current) return
        if (forced) {
          const forceConfirmed = await confirm({
            title: `再次确认强制更新到 ${preview.releaseTag}？`,
            body: (
              <>
                <div className="modalLead">Dockrev 无法证明当前配置标签过去指向过这个版本；强制更新将使用同一镜像仓库中原始 Release tag 当前解析到的摘要。</div>
                <div className="modalKvGrid">
                  <div className="modalKvLabel">镜像仓库</div>
                  <div className="modalKvValue"><Mono>{preview.imageRepo}</Mono></div>
                  <div className="modalKvLabel">配置标签</div>
                  <div className="modalKvValue"><Mono>{preview.configuredTag}</Mono></div>
                  <div className="modalKvLabel">后续自动更新</div>
                  <div className="modalKvValue">{autoUpdateNotice}</div>
                </div>
              </>
            ),
            confirmText: '确认强制更新',
            cancelText: '返回',
            confirmVariant: 'danger',
            badgeText: null,
          })
          if (!forceConfirmed || generation !== pageGenerationRef.current) return
        }
        const response = await triggerServiceVersionUpdate(service.id, {
          ...preview,
          forceConfirmed: forced,
          backupMode: 'inherit',
        })
        if (generation !== pageGenerationRef.current) return
        setNotice({ jobId: response.jobId, kind: 'update' })
        if (applyActionKey) trackJob(applyActionKey, response.jobId, 'queued', preview.releaseTag)
        void refreshLifecycleStatus(generation).catch(() => undefined)
      } catch (error: unknown) {
        if (generation !== pageGenerationRef.current) return
        if (error instanceof ApiError) {
          if (error.status === 401) setError('需要登录/鉴权（Forward Auth）')
          else if (error.status === 409) {
            const existingJobId = conflictingJobId(error)
            if (existingJobId) navigate({ name: 'job', jobId: existingJobId })
            else {
              setError('服务状态或版本摘要已变化，请重新预检后再试。')
              await requestRefresh()
            }
          } else setError(error.message)
        } else setError(errorMessage(error))
      } finally {
        if (submissionToken) {
          const target = submittingTokensRef.current.get(submissionToken)
          if (target) {
            submittingTokensRef.current.delete(submissionToken)
            endSubmitting(target)
          }
        }
      }
    })()
  }, [
    applyActionKey, applyActiveJob, beginSubmitting, confirm, endSubmitting,
    pageGeneration, pageGenerationRef, refreshLifecycleStatus, requestRefresh,
    service, setError, setNotice, submittingTokensRef, trackJob,
  ])
}
