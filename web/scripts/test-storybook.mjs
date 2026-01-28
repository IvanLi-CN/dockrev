import { access, readFile } from 'node:fs/promises'
import http from 'node:http'
import path from 'node:path'

const DEFAULT_OUTDIR = path.resolve(process.cwd(), 'storybook-static')
const DEFAULT_PORT = 50887

function parsePort(value, fallback) {
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function parseArgs(argv) {
  const out = { url: null, passthrough: [] }
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (a === '--url') {
      out.url = argv[i + 1] ?? null
      i++
      continue
    }
    out.passthrough.push(a)
  }
  return out
}

function contentType(filePath) {
  const ext = path.extname(filePath).toLowerCase()
  if (ext === '.html') return 'text/html; charset=utf-8'
  if (ext === '.js' || ext === '.mjs') return 'text/javascript; charset=utf-8'
  if (ext === '.css') return 'text/css; charset=utf-8'
  if (ext === '.json') return 'application/json; charset=utf-8'
  if (ext === '.svg') return 'image/svg+xml'
  if (ext === '.png') return 'image/png'
  if (ext === '.jpg' || ext === '.jpeg') return 'image/jpeg'
  if (ext === '.woff') return 'font/woff'
  if (ext === '.woff2') return 'font/woff2'
  return 'application/octet-stream'
}

async function waitForHttpOk(url, timeoutMs = 60_000) {
  const startedAt = Date.now()
  while (Date.now() - startedAt < timeoutMs) {
    try {
      const resp = await fetch(url, { method: 'GET' })
      if (resp.ok) return
    } catch {
      // ignore until timeout
    }
    await new Promise((r) => setTimeout(r, 500))
  }
  throw new Error(`Timed out waiting for ${url}`)
}

async function ensureStaticBuild() {
  try {
    await access(path.join(DEFAULT_OUTDIR, 'index.html'))
    await access(path.join(DEFAULT_OUTDIR, 'iframe.html'))
    return
  } catch {
    console.error('Missing storybook-static build. Run: bun run build-storybook')
    process.exit(1)
  }
}

function startStaticServer({ port }) {
  const sockets = new Set()
  const server = http.createServer(async (req, res) => {
    const reqUrl = new URL(req.url ?? '/', `http://${req.headers.host ?? '127.0.0.1'}`)
    const pathname = reqUrl.pathname === '/' ? '/index.html' : reqUrl.pathname
    const filePath = path.resolve(DEFAULT_OUTDIR, `.${pathname}`)
    if (!filePath.startsWith(DEFAULT_OUTDIR)) {
      res.statusCode = 403
      res.end('Forbidden')
      return
    }

    try {
      const body = await readFile(filePath)
      res.statusCode = 200
      res.setHeader('Content-Type', contentType(filePath))
      res.end(body)
    } catch {
      res.statusCode = 404
      res.end('Not found')
    }
  })

  server.on('connection', (socket) => {
    sockets.add(socket)
    socket.on('close', () => sockets.delete(socket))
  })

  const listen = () =>
    new Promise((resolve, reject) => {
      const onError = (err) => {
        server.off('error', onError)
        reject(err)
      }
      server.on('error', onError)
      server.listen(port, '127.0.0.1', () => {
        server.off('error', onError)
        resolve()
      })
    })

  const cleanup = () =>
    new Promise((resolve) => {
      for (const s of sockets) s.destroy()
      server.close(() => resolve())
    })

  return { listen, cleanup }
}

async function getStoryIds(baseUrl) {
  const base = normalizeBaseUrl(baseUrl)
  const resp = await fetch(new URL('index.json', base))
  if (!resp.ok) {
    throw new Error(`Failed to fetch Storybook index.json: ${resp.status} ${resp.statusText}`)
  }
  const json = await resp.json()
  const entries = (json && typeof json === 'object' && json.entries) || {}
  if (!entries || typeof entries !== 'object') return []
  return Object.values(entries)
    .filter((e) => e && typeof e === 'object' && e.type === 'story' && typeof e.id === 'string')
    .map((e) => e.id)
}

function normalizeBaseUrl(input) {
  const url = new URL(input)
  url.search = ''
  url.hash = ''
  if (url.pathname.endsWith('/iframe.html') || url.pathname.endsWith('/index.html')) {
    url.pathname = url.pathname.replace(/[^/]+$/, '')
  }
  if (!url.pathname.endsWith('/')) url.pathname += '/'
  return url.toString()
}

async function runSmoke({ baseUrl, storyIds, browser }) {
  if (storyIds.length === 0) {
    throw new Error(
      'No stories discovered from index.json. Storybook may be misconfigured or the index schema may have changed.'
    )
  }
  console.log(`Testing ${storyIds.length} stories...`)
  const failures = []

  for (const id of storyIds) {
    const page = await browser.newPage()
    const pageErrors = []
    page.on('pageerror', (err) => pageErrors.push(err))

    try {
      const base = normalizeBaseUrl(baseUrl)
      const url = new URL('iframe.html', base)
      url.searchParams.set('id', id)
      url.searchParams.set('viewMode', 'story')

      await page.goto(url.toString(), { waitUntil: 'domcontentloaded' })
      await page.waitForFunction(() => document.body.classList.contains('sb-show-main'), null, {
        timeout: 60_000,
      })

      if (pageErrors.length > 0) {
        failures.push({ id, error: pageErrors[0] })
      }
    } catch (error) {
      failures.push({ id, error })
    } finally {
      await page.close().catch(() => {})
    }
  }

  if (failures.length > 0) {
    console.error(`Failed ${failures.length}/${storyIds.length} stories:`)
    for (const f of failures.slice(0, 20)) {
      console.error(`- ${f.id}: ${String(f.error?.message ?? f.error)}`)
    }
    if (failures.length > 20) {
      console.error(`...and ${failures.length - 20} more`)
    }
    throw new Error(`Storybook smoke test failed (${failures.length}/${storyIds.length}).`)
  }

  console.log('All stories passed.')
}

async function runInteractive({ baseUrl, browser }) {
  const base = normalizeBaseUrl(baseUrl)

  const openStory = async (id) => {
    const page = await browser.newPage()
    page.on('dialog', (d) => d.accept().catch(() => {}))
    const url = new URL('iframe.html', base)
    url.searchParams.set('id', id)
    url.searchParams.set('viewMode', 'story')
    await page.goto(url.toString(), { waitUntil: 'domcontentloaded' })
    await page.waitForFunction(() => document.body.classList.contains('sb-show-main'), null, { timeout: 60_000 })
    return page
  }

  // 1) Disabled state (no candidates): "更新全部" must be disabled.
  {
    const page = await openStory('pages-overviewpage--no-candidates-but-has-services')
    try {
      const btn = page.getByRole('button', { name: '更新全部' })
      await btn.waitFor({ timeout: 10_000 })
      await page.waitForFunction(
        () => {
          const el = Array.from(document.querySelectorAll('button')).find((b) => b.textContent?.trim() === '更新全部')
          return Boolean(el && el.disabled)
        },
        null,
        { timeout: 10_000 },
      )
      const disabled = await btn.isDisabled()
      if (!disabled) throw new Error('Expected "更新全部" to be disabled in no-candidates scenario.')
    } finally {
      await page.close().catch(() => {})
    }
  }

  // 2) Request parameters: clicking "更新全部" must call POST /api/updates with fixed fields.
  {
    const page = await openStory('pages-overviewpage--default')
    try {
      const btn = page.getByRole('button', { name: '更新全部' })
      await btn.waitFor({ timeout: 10_000 })
      // Wait for page data fetch to populate counts and enable the button.
      await page.waitForFunction(
        () => {
          const el = Array.from(document.querySelectorAll('button')).find((b) => b.textContent?.trim() === '更新全部')
          return Boolean(el && !el.disabled)
        },
        null,
        { timeout: 10_000 },
      )
      await btn.click()

      // The app uses a custom confirm dialog (not the browser's built-in confirm).
      const modal = page.getByRole('dialog')
      await modal.waitFor({ timeout: 10_000 })
      await modal.getByRole('button', { name: '执行更新' }).click()

      await page.waitForFunction(() => Boolean(globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest), null, {
        timeout: 10_000,
      })
      const req = await page.evaluate(() => globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest ?? null)
      if (!req || typeof req !== 'object') throw new Error('No update request recorded in mock API.')

      const scope = req.scope
      const mode = req.mode
      const allowArchMismatch = req.allowArchMismatch
      const backupMode = req.backupMode
      const reason = req.reason

      if (scope !== 'all') throw new Error(`Expected scope=all, got ${String(scope)}`)
      if (mode !== 'apply') throw new Error(`Expected mode=apply, got ${String(mode)}`)
      if (allowArchMismatch !== false) throw new Error(`Expected allowArchMismatch=false, got ${String(allowArchMismatch)}`)
      if (backupMode !== 'inherit') throw new Error(`Expected backupMode=inherit, got ${String(backupMode)}`)
      if (reason !== 'ui') throw new Error(`Expected reason=ui, got ${String(reason)}`)

      await page.getByText('已创建更新任务').waitFor({ timeout: 5_000 })
    } finally {
      await page.close().catch(() => {})
    }
  }
}

async function main() {
  const { url: cliUrl, passthrough } = parseArgs(process.argv.slice(2))
  const targetUrl = cliUrl ?? process.env.TARGET_URL ?? null

  if (targetUrl) {
    if (passthrough.length > 0) {
      console.error('Only --url is supported for now; extra args are not accepted.')
      process.exit(2)
    }
    const { chromium } = await import('playwright')
    const browser = await chromium.launch()
    const storyIds = await getStoryIds(targetUrl)
    try {
      await runSmoke({ baseUrl: targetUrl, storyIds, browser })
      await runInteractive({ baseUrl: targetUrl, browser })
    } finally {
      await browser.close().catch(() => {})
    }
    return
  }

  await ensureStaticBuild()
  const port = parsePort(process.env.DOCKREV_TEST_STORYBOOK_PORT, DEFAULT_PORT)
  const server = startStaticServer({ port })
  try {
    await server.listen()
  } catch (error) {
    if (error && typeof error === 'object' && error.code === 'EADDRINUSE') {
      console.error(
        `Port ${port} is already in use. Set DOCKREV_TEST_STORYBOOK_PORT or pass --url/TARGET_URL.`
      )
      process.exit(1)
    }
    throw error
  }

  try {
    const localUrl = `http://127.0.0.1:${port}`
    await waitForHttpOk(localUrl)
    if (passthrough.length > 0) {
      console.error('Only --url is supported for now; extra args are not accepted.')
      process.exit(2)
    }
    const { chromium } = await import('playwright')
    const browser = await chromium.launch()
    const storyIds = await getStoryIds(localUrl)
    try {
      await runSmoke({ baseUrl: localUrl, storyIds, browser })
      await runInteractive({ baseUrl: localUrl, browser })
    } finally {
      await browser.close().catch(() => {})
    }
  } finally {
    await server.cleanup()
  }
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
