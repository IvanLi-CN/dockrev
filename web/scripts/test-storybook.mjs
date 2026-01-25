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
      server.close(() => resolve())
    })

  return { listen, cleanup }
}

async function getStoryIds(baseUrl) {
  const resp = await fetch(new URL('/index.json', baseUrl))
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

async function runSmoke({ baseUrl, storyIds }) {
  const { chromium } = await import('playwright')
  const browser = await chromium.launch()
  try {
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
        const url = new URL('/iframe.html', baseUrl)
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
      process.exit(1)
    }

    console.log('All stories passed.')
  } finally {
    await browser.close().catch(() => {})
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
    const storyIds = await getStoryIds(targetUrl)
    await runSmoke({ baseUrl: targetUrl, storyIds })
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
    const storyIds = await getStoryIds(localUrl)
    await runSmoke({ baseUrl: localUrl, storyIds })
  } finally {
    await server.cleanup()
  }
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
