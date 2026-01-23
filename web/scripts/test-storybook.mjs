import { spawn } from 'node:child_process'
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

function run(command, args, { silent = false } = {}) {
  return new Promise((resolve) => {
    const child = spawn(command, args, {
      stdio: silent ? 'ignore' : 'inherit',
      shell: process.platform === 'win32',
    })
    child.on('exit', (code) => resolve(code ?? 1))
  })
}

async function ensureStaticBuild() {
  try {
    await access(path.join(DEFAULT_OUTDIR, 'index.html'))
    await access(path.join(DEFAULT_OUTDIR, 'iframe.html'))
    return
  } catch {
    const code = await run('storybook', ['build', '--quiet'])
    if (code !== 0) process.exit(code)
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

async function main() {
  const { url: cliUrl, passthrough } = parseArgs(process.argv.slice(2))
  const targetUrl = cliUrl ?? process.env.TARGET_URL ?? null

  if (targetUrl) {
    const code = await run('test-storybook', ['--url', targetUrl, ...passthrough])
    process.exit(code)
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
    const code = await run('test-storybook', ['--url', localUrl, ...passthrough])
    process.exit(code)
  } finally {
    await server.cleanup()
  }
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
