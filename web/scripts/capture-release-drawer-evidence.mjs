import http from 'node:http'
import net from 'node:net'
import path from 'node:path'
import { readFile } from 'node:fs/promises'
import { chromium } from 'playwright'

const repoRoot = '/Users/ivan/.codex/worktrees/0c96/dockrev'
const storybookDir = path.join(repoRoot, 'web', 'storybook-static')
const shots = [
  {
    id: 'components-githubreleasedrawer--anonymous-located',
    raw: path.join(repoRoot, 'docs/specs/4fhgd-github-release-drawer/assets/release-drawer-scrollable.raw.png'),
    waitFor: '[data-release-tag="1.39.5"]',
    hoverInfo: true,
    drawerHeight: 'min(860px, calc(100vh - 32px))',
    scrollRegionHeight: 'min(680px, calc(100vh - 188px))',
  },
  {
    id: 'components-githubreleasedrawer--pat-authenticated-short-list',
    raw: path.join(repoRoot, 'docs/specs/4fhgd-github-release-drawer/assets/release-drawer-short-list.raw.png'),
    waitFor: '[data-release-drawer="true"]',
    hoverInfo: false,
    drawerHeight: 'min(860px, calc(100vh - 32px))',
    scrollRegionHeight: 'auto',
  },
]

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

async function findPort() {
  return await new Promise((resolve, reject) => {
    const probe = net.createServer()
    probe.on('error', reject)
    probe.listen(0, '127.0.0.1', () => {
      const address = probe.address()
      const port = typeof address === 'object' && address ? address.port : 0
      probe.close((error) => (error ? reject(error) : resolve(port)))
    })
  })
}

const server = http.createServer(async (req, res) => {
  const reqUrl = new URL(req.url ?? '/', 'http://127.0.0.1')
  const pathname = reqUrl.pathname === '/' ? '/index.html' : reqUrl.pathname
  const filePath = path.resolve(storybookDir, '.' + pathname)
  if (!filePath.startsWith(storybookDir)) {
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

const port = await findPort()
await new Promise((resolve) => server.listen(port, '127.0.0.1', resolve))
const baseUrl = 'http://127.0.0.1:' + String(port)

const browser = await chromium.launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 920 }, deviceScaleFactor: 2 })

try {
  for (const shot of shots) {
    const page = await context.newPage()
    const url = new URL('/iframe.html', baseUrl)
    url.searchParams.set('id', shot.id)
    url.searchParams.set('viewMode', 'story')
    await page.goto(url.toString(), { waitUntil: 'domcontentloaded' })
    await page.waitForFunction(() => {
      const root = document.querySelector('#storybook-root, #root')
      return Boolean(root && root.childElementCount > 0)
    }, null, { timeout: 60_000 })
    const target = page.locator(shot.waitFor)
    await target.waitFor({ timeout: 60_000 })
    await target.scrollIntoViewIfNeeded().catch(() => {})
    await page.waitForTimeout(600)
    if (shot.hoverInfo) {
      const info = page.locator('[data-release-drawer-info-trigger="true"]')
      await info.waitFor({ timeout: 60_000 })
      await info.hover()
      await page.locator('[data-release-drawer-info-tooltip="true"]').waitFor({ timeout: 60_000 })
      await page.waitForTimeout(240)
    }
    const shell = page.locator('[data-release-drawer-story-shell="true"]')
    await shell.waitFor({ timeout: 60_000 })
    await shell.screenshot({ path: shot.raw })
    await page.close()
    console.log('saved ' + path.relative(repoRoot, shot.raw))
  }
} finally {
  await context.close().catch(() => {})
  await browser.close().catch(() => {})
  await new Promise((resolve) => server.close(resolve))
}
