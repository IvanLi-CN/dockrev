import './App.css'
import { useEffect, useState } from 'react'
import {
  createIgnore,
  deleteIgnore,
  getJob,
  getNotifications,
  getSettings,
  getStack,
  listIgnores,
  listJobs,
  listStacks,
  putNotifications,
  putSettings,
  registerStack,
  triggerCheck,
  triggerUpdate,
  type IgnoreRule,
  type JobListItem,
  type NotificationConfig,
  type Service,
  type SettingsResponse,
  type StackDetail,
  type StackListItem,
} from './api'

type Tab = 'stacks' | 'ignores' | 'jobs' | 'settings'

function App() {
  const [tab, setTab] = useState<Tab>('stacks')

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <div className="brand-title">Dockrev</div>
          <div className="brand-subtitle">Docker/Compose 更新管理器（MVP）</div>
        </div>

        <nav className="tabs">
          <TabButton active={tab === 'stacks'} onClick={() => setTab('stacks')}>
            Stacks
          </TabButton>
          <TabButton active={tab === 'ignores'} onClick={() => setTab('ignores')}>
            Ignores
          </TabButton>
          <TabButton active={tab === 'jobs'} onClick={() => setTab('jobs')}>
            Jobs
          </TabButton>
          <TabButton active={tab === 'settings'} onClick={() => setTab('settings')}>
            Settings
          </TabButton>
        </nav>
      </header>

      <main className="main">
        {tab === 'stacks' && <StacksView />}
        {tab === 'ignores' && <IgnoresView />}
        {tab === 'jobs' && <JobsView />}
        {tab === 'settings' && <SettingsView />}
      </main>
    </div>
  )
}

export default App

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function TabButton(props: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button className={props.active ? 'tab active' : 'tab'} onClick={props.onClick}>
      {props.children}
    </button>
  )
}

function StacksView() {
  const [stacks, setStacks] = useState<StackListItem[]>([])
  const [expanded, setExpanded] = useState<Record<string, boolean>>({})
  const [details, setDetails] = useState<Record<string, StackDetail | undefined>>({})
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const [name, setName] = useState('')
  const [composeFilesText, setComposeFilesText] = useState('')
  const [envFile, setEnvFile] = useState('')

  async function refresh() {
    setError(null)
    try {
      const data = await listStacks()
      setStacks(data)
    } catch (e: unknown) {
      setError(errorMessage(e))
    }
  }

  useEffect(() => {
    void refresh()
  }, [])

  async function toggleStack(stackId: string) {
    setExpanded((prev) => ({ ...prev, [stackId]: !prev[stackId] }))
    if (details[stackId]) return
    try {
      const d = await getStack(stackId)
      setDetails((prev) => ({ ...prev, [stackId]: d }))
    } catch (e: unknown) {
      setError(errorMessage(e))
    }
  }

  async function doCheck(stackId: string) {
    setBusy(true)
    setError(null)
    try {
      await triggerCheck('stack', stackId)
      await refresh()
      const d = await getStack(stackId)
      setDetails((prev) => ({ ...prev, [stackId]: d }))
    } catch (e: unknown) {
      setError(errorMessage(e))
    } finally {
      setBusy(false)
    }
  }

	  async function doUpdate(scope: 'stack' | 'service', stackId: string, serviceId?: string) {
	    setBusy(true)
	    setError(null)
	    try {
	      await triggerUpdate({
	        scope,
	        stackId,
	        serviceId,
	        mode: 'apply',
	        allowArchMismatch: false,
	        backupMode: 'inherit',
	      })
	      await refresh()
	    } catch (e: unknown) {
	      setError(errorMessage(e))
	    } finally {
	      setBusy(false)
	    }
	  }

  async function doRegister() {
    setBusy(true)
    setError(null)
    try {
      const files = composeFilesText
        .split('\n')
        .map((s) => s.trim())
        .filter(Boolean)
      if (!name.trim() || files.length === 0) {
        throw new Error('请填写 name 与至少 1 个 composeFiles（绝对路径）')
      }
	      await registerStack({ name: name.trim(), composeFiles: files, envFile: envFile.trim() || null })
	      setName('')
	      setComposeFilesText('')
	      setEnvFile('')
	      await refresh()
	    } catch (e: unknown) {
	      setError(errorMessage(e))
	    } finally {
	      setBusy(false)
	    }
	  }

  return (
    <div className="grid">
      <section className="panel">
        <h2>注册 Stack</h2>
        <div className="form">
          <label>
            Name
            <input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-app" />
          </label>
          <label>
            Compose files（每行 1 个，容器内绝对路径）
            <textarea
              value={composeFilesText}
              onChange={(e) => setComposeFilesText(e.target.value)}
              placeholder="/srv/compose/app/docker-compose.yml"
              rows={3}
            />
          </label>
          <label>
            Env file（可选）
            <input value={envFile} onChange={(e) => setEnvFile(e.target.value)} placeholder="/srv/compose/app/.env" />
          </label>
          <div className="row">
            <button disabled={busy} onClick={() => void doRegister()}>
              Register
            </button>
            <button disabled={busy} onClick={() => void refresh()}>
              Refresh
            </button>
          </div>
          {error && <div className="error">{error}</div>}
        </div>
      </section>

      <section className="panel">
        <h2>Stacks</h2>
        <div className="list">
          {stacks.length === 0 && <div className="muted">暂无 stacks（先注册一个）</div>}
          {stacks.map((s) => (
            <div key={s.id} className="card">
              <div className="card-head">
                <button className="link" onClick={() => void toggleStack(s.id)}>
                  {expanded[s.id] ? '▾' : '▸'} {s.name}
                </button>
                <div className="pill">{s.updates} updates</div>
              </div>
              <div className="card-meta">
                <span>ID: {s.id}</span>
                <span>Services: {s.services}</span>
                <span>Last check: {s.lastCheckAt}</span>
              </div>
              <div className="row">
                <button disabled={busy} onClick={() => void doCheck(s.id)}>
                  Check
                </button>
                <button disabled={busy} onClick={() => void doUpdate('stack', s.id)}>
                  Update stack
                </button>
              </div>
              {expanded[s.id] && (
                <div className="services">
                  <StackServices
                    stackId={s.id}
                    detail={details[s.id]}
                    onUpdateService={(serviceId) => void doUpdate('service', s.id, serviceId)}
                  />
                </div>
              )}
            </div>
          ))}
        </div>
      </section>
    </div>
  )
}

function StackServices(props: {
  stackId: string
  detail?: StackDetail
  onUpdateService: (serviceId: string) => void
}) {
  const services = props.detail?.services || []
  if (!props.detail) return <div className="muted">加载中…</div>

  return (
    <div className="services-list">
      {services.length === 0 && <div className="muted">该 stack 暂无 services</div>}
      {services.map((svc) => (
        <ServiceRow key={svc.id} service={svc} onUpdate={() => props.onUpdateService(svc.id)} />
      ))}
    </div>
  )
}

function ServiceRow(props: { service: Service; onUpdate: () => void }) {
  const svc = props.service
  const canUpdate =
    !!svc.candidate && !svc.ignore?.matched && svc.candidate.archMatch === 'match' && svc.candidate.tag !== svc.image.tag

  const candidateText = svc.candidate
    ? `${svc.candidate.tag} (${svc.candidate.archMatch})`
    : '—'

  return (
    <div className="service-row">
      <div className="service-main">
        <div className="service-name">{svc.name}</div>
        <div className="service-meta">
          <span className="mono">{svc.image.ref}</span>
          {svc.image.digest && <span className="mono">digest: {svc.image.digest}</span>}
        </div>
      </div>
      <div className="service-side">
        <div className={canUpdate ? 'ok' : 'muted'}>candidate: {candidateText}</div>
        {svc.ignore?.matched && <div className="warn">ignored: {svc.ignore.reason}</div>}
        <button disabled={!canUpdate} onClick={props.onUpdate}>
          Update
        </button>
      </div>
    </div>
  )
}

function IgnoresView() {
  const [rules, setRules] = useState<IgnoreRule[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const [serviceId, setServiceId] = useState('')
  const [kind, setKind] = useState('prefix')
  const [value, setValue] = useState('5.3.')
  const [note, setNote] = useState('')

	  async function refresh() {
	    setError(null)
	    try {
	      setRules(await listIgnores())
	    } catch (e: unknown) {
	      setError(errorMessage(e))
	    }
	  }

  useEffect(() => {
    void refresh()
  }, [])

  async function doCreate() {
    setBusy(true)
    setError(null)
    try {
	      if (!serviceId.trim() || !value.trim()) throw new Error('请填写 serviceId 与 value')
	      await createIgnore({ enabled: true, serviceId: serviceId.trim(), kind, value: value.trim(), note: note.trim() })
	      setNote('')
	      await refresh()
	    } catch (e: unknown) {
	      setError(errorMessage(e))
	    } finally {
	      setBusy(false)
	    }
	  }

  async function doDelete(id: string) {
    setBusy(true)
	    setError(null)
	    try {
	      await deleteIgnore(id)
	      await refresh()
	    } catch (e: unknown) {
	      setError(errorMessage(e))
	    } finally {
	      setBusy(false)
	    }
	  }

  return (
    <div className="grid">
      <section className="panel">
        <h2>创建 Ignore 规则</h2>
        <div className="form">
          <label>
            Service ID
            <input value={serviceId} onChange={(e) => setServiceId(e.target.value)} placeholder="svc_..." />
          </label>
          <label>
            Kind
            <select value={kind} onChange={(e) => setKind(e.target.value)}>
              <option value="exact">exact</option>
              <option value="prefix">prefix</option>
              <option value="regex">regex</option>
              <option value="semver">semver</option>
            </select>
          </label>
          <label>
            Value
            <input value={value} onChange={(e) => setValue(e.target.value)} />
          </label>
          <label>
            Note（可选）
            <input value={note} onChange={(e) => setNote(e.target.value)} />
          </label>
          <div className="row">
            <button disabled={busy} onClick={() => void doCreate()}>
              Create
            </button>
            <button disabled={busy} onClick={() => void refresh()}>
              Refresh
            </button>
          </div>
          {error && <div className="error">{error}</div>}
        </div>
      </section>

      <section className="panel">
        <h2>规则列表</h2>
        <div className="list">
          {rules.map((r) => (
            <div key={r.id} className="card">
              <div className="card-head">
                <div className="mono">{r.id}</div>
                <button disabled={busy} onClick={() => void doDelete(r.id)}>
                  Delete
                </button>
              </div>
              <div className="card-meta">
                <span>enabled: {String(r.enabled)}</span>
                <span>service: {r.scope.serviceId}</span>
                <span>
                  match: {r.match.kind}={r.match.value}
                </span>
              </div>
              {r.note && <div className="muted">note: {r.note}</div>}
            </div>
          ))}
          {rules.length === 0 && <div className="muted">暂无规则</div>}
        </div>
      </section>
    </div>
  )
}

function JobsView() {
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const [logs, setLogs] = useState<string[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

	  async function refresh() {
	    setError(null)
	    try {
	      setJobs(await listJobs())
	    } catch (e: unknown) {
	      setError(errorMessage(e))
	    }
	  }

  useEffect(() => {
    void refresh()
  }, [])

  useEffect(() => {
    if (!selected) return
    setBusy(true)
    void (async () => {
	      try {
	        const job = await getJob(selected)
	        setLogs(job.logs.map((l) => `${l.ts} [${l.level}] ${l.msg}`))
	      } catch (e: unknown) {
	        setError(errorMessage(e))
	      } finally {
	        setBusy(false)
	      }
	    })()
	  }, [selected])

  return (
    <div className="grid">
      <section className="panel">
        <h2>Jobs</h2>
        <div className="row">
          <button disabled={busy} onClick={() => void refresh()}>
            Refresh
          </button>
        </div>
        {error && <div className="error">{error}</div>}
        <div className="list">
          {jobs.map((j) => (
            <div key={j.id} className={selected === j.id ? 'card selected' : 'card'}>
              <button className="link" onClick={() => setSelected(j.id)}>
                {j.type} / {j.scope} — {j.status}
              </button>
              <div className="card-meta">
                <span className="mono">{j.id}</span>
                <span>created: {j.createdAt}</span>
              </div>
            </div>
          ))}
          {jobs.length === 0 && <div className="muted">暂无 jobs</div>}
        </div>
      </section>

      <section className="panel">
        <h2>Logs</h2>
        {!selected && <div className="muted">选择一个 job 查看日志</div>}
        {selected && <div className="mono">job: {selected}</div>}
        <pre className="logs">{logs.join('\n')}</pre>
      </section>
    </div>
  )
}

function SettingsView() {
  const [settings, setSettings] = useState<SettingsResponse | null>(null)
  const [notifications, setNotifications] = useState<NotificationConfig | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const backup = settings?.backup
  const auth = settings?.auth

	  async function refresh() {
	    setError(null)
	    try {
	      setSettings(await getSettings())
	      setNotifications(await getNotifications())
	    } catch (e: unknown) {
	      setError(errorMessage(e))
	    }
	  }

  useEffect(() => {
    void refresh()
  }, [])

  async function saveBackup() {
    if (!backup) return
	    setBusy(true)
	    setError(null)
	    try {
	      await putSettings(backup)
	      await refresh()
	    } catch (e: unknown) {
	      setError(errorMessage(e))
	    } finally {
	      setBusy(false)
	    }
	  }

  async function saveNotifications() {
    if (!notifications) return
	    setBusy(true)
	    setError(null)
	    try {
	      await putNotifications(notifications)
	      await refresh()
	    } catch (e: unknown) {
	      setError(errorMessage(e))
	    } finally {
	      setBusy(false)
	    }
	  }

  return (
    <div className="grid">
      <section className="panel">
        <h2>Settings</h2>
        <div className="row">
          <button disabled={busy} onClick={() => void refresh()}>
            Refresh
          </button>
          <button disabled={busy} onClick={() => void saveBackup()}>
            Save backup
          </button>
        </div>
        {error && <div className="error">{error}</div>}
        {!settings && <div className="muted">加载中…</div>}
        {settings && (
          <>
            <div className="form">
              <label>
                backup.enabled
                <input
                  type="checkbox"
                  checked={backup?.enabled || false}
                  onChange={(e) =>
                    setSettings((s) => (s ? { ...s, backup: { ...s.backup, enabled: e.target.checked } } : s))
                  }
                />
              </label>
              <label>
                backup.requireSuccess
                <input
                  type="checkbox"
                  checked={backup?.requireSuccess || false}
                  onChange={(e) =>
                    setSettings((s) =>
                      s ? { ...s, backup: { ...s.backup, requireSuccess: e.target.checked } } : s,
                    )
                  }
                />
              </label>
              <label>
                backup.baseDir
                <input
                  value={backup?.baseDir || ''}
                  onChange={(e) => setSettings((s) => (s ? { ...s, backup: { ...s.backup, baseDir: e.target.value } } : s))}
                />
              </label>
              <label>
                backup.skipTargetsOverBytes
                <input
                  type="number"
                  value={backup?.skipTargetsOverBytes || 0}
                  onChange={(e) =>
                    setSettings((s) =>
                      s
                        ? { ...s, backup: { ...s.backup, skipTargetsOverBytes: Number(e.target.value) } }
                        : s,
                    )
                  }
                />
              </label>
            </div>
            <div className="muted">
              auth.forwardHeaderName: <span className="mono">{auth?.forwardHeaderName}</span> / allowAnonymousInDev:{' '}
              {String(auth?.allowAnonymousInDev)}
            </div>
          </>
        )}
      </section>

      <section className="panel">
        <h2>Notifications</h2>
        <div className="row">
          <button disabled={busy} onClick={() => void saveNotifications()}>
            Save notifications
          </button>
        </div>
        {!notifications && <div className="muted">加载中…</div>}
        {notifications && (
          <div className="form">
            <label>
              webhook.enabled
              <input
                type="checkbox"
                checked={notifications.webhook.enabled}
                onChange={(e) =>
                  setNotifications((n) => (n ? { ...n, webhook: { ...n.webhook, enabled: e.target.checked } } : n))
                }
              />
            </label>
            <label>
              webhook.url（敏感值读取会脱敏）
              <input
                value={notifications.webhook.url || ''}
                onChange={(e) =>
                  setNotifications((n) => (n ? { ...n, webhook: { ...n.webhook, url: e.target.value } } : n))
                }
              />
            </label>
            <label>
              email.enabled
              <input
                type="checkbox"
                checked={notifications.email.enabled}
                onChange={(e) =>
                  setNotifications((n) => (n ? { ...n, email: { ...n.email, enabled: e.target.checked } } : n))
                }
              />
            </label>
            <label>
              email.smtpUrl（敏感值读取会脱敏）
              <input
                value={notifications.email.smtpUrl || ''}
                onChange={(e) =>
                  setNotifications((n) => (n ? { ...n, email: { ...n.email, smtpUrl: e.target.value } } : n))
                }
              />
            </label>
          </div>
        )}
      </section>
    </div>
  )
}
