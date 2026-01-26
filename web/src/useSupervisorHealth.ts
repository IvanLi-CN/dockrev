import { useCallback, useEffect, useMemo, useState } from 'react'
import { selfUpgradeBaseUrl } from './runtimeConfig'

type HealthState =
  | { status: 'idle' }
  | { status: 'checking'; lastOkAt?: string; lastErrorAt?: string; lastError?: string }
  | { status: 'ok'; okAt: string }
  | { status: 'offline'; errorAt: string; error: string; lastOkAt?: string }

function resolveUrl(path: string, base: string): string {
  // Support absolute URLs and same-origin paths.
  const b = new URL(base, window.location.href)
  const p = path.startsWith('/') ? path.slice(1) : path
  return new URL(p, b.toString()).toString()
}

async function fetchWithTimeout(url: string, timeoutMs: number): Promise<Response> {
  const ctrl = new AbortController()
  const t = setTimeout(() => ctrl.abort(), timeoutMs)
  try {
    return await fetch(url, { method: 'GET', signal: ctrl.signal })
  } finally {
    clearTimeout(t)
  }
}

export function useSupervisorHealth() {
  const baseUrl = useMemo(() => selfUpgradeBaseUrl(), [])
  const [state, setState] = useState<HealthState>({ status: 'idle' })

  const check = useCallback(async () => {
    setState((prev) => {
      if (prev.status === 'ok') return { status: 'checking', lastOkAt: prev.okAt }
      if (prev.status === 'offline') return { status: 'checking', lastOkAt: prev.lastOkAt, lastErrorAt: prev.errorAt, lastError: prev.error }
      return { status: 'checking' }
    })

    // Probe an authenticated endpoint to avoid reporting "ok" when the supervisor console/API would 401.
    try {
      const url = resolveUrl('self-upgrade', baseUrl)
      const resp = await fetchWithTimeout(url, 1200)
      if (resp.status === 401) throw new Error('需要登录/鉴权（forward header）')
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
      setState({ status: 'ok', okAt: new Date().toISOString() })
    } catch (e: unknown) {
      const msgRaw = e instanceof Error ? e.message : String(e)
      const msg = msgRaw.includes('Invalid URL') ? `自我升级地址配置无效：${msgRaw}` : msgRaw
      setState((prev) => {
        const lastOkAt = prev.status === 'checking' ? prev.lastOkAt : prev.status === 'ok' ? prev.okAt : prev.status === 'offline' ? prev.lastOkAt : undefined
        return { status: 'offline', errorAt: new Date().toISOString(), error: msg, lastOkAt }
      })
    }
  }, [baseUrl])

  useEffect(() => {
    void check()
  }, [check])

  return { baseUrl, state, check }
}
