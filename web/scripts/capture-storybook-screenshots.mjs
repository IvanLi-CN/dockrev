import path from 'node:path'
import { access, mkdir, readFile } from 'node:fs/promises'
import http from 'node:http'
import net from 'node:net'
import { chromium } from 'playwright'

const DEFAULT_PORT = 50886
const DEFAULT_OUTDIR = path.resolve(process.cwd(), 'storybook-static')
const STORY_TIMEOUT_MS = 20_000

function parseArgs(argv) {
  const out = { url: null, outdir: null }
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (a === '--url') {
      out.url = argv[i + 1] ?? null
      i++
      continue
    }
    if (a === '--outdir') {
      out.outdir = argv[i + 1] ?? null
      i++
      continue
    }
  }
  return out
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

function iframeUrl(baseUrl, storyId) {
  const base = normalizeBaseUrl(baseUrl)
  const url = new URL('iframe.html', base)
  url.searchParams.set('id', storyId)
  url.searchParams.set('viewMode', 'story')
  return url.toString()
}

async function findAvailablePort(preferredPort) {
  return await new Promise((resolve, reject) => {
    const probe = net.createServer()
    let retriedWithRandomPort = false

    const handleError = (error) => {
      if (!retriedWithRandomPort && preferredPort !== 0 && error && typeof error === 'object' && error.code === 'EADDRINUSE') {
        retriedWithRandomPort = true
        probe.listen(0, '127.0.0.1')
        return
      }
      reject(error)
    }

    probe.on('error', handleError)
    probe.listen(preferredPort, '127.0.0.1', () => {
      const address = probe.address()
      const port = typeof address === 'object' && address ? address.port : preferredPort
      probe.close((closeError) => {
        if (closeError) reject(closeError)
        else resolve(port)
      })
    })
  })
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
    await new Promise((resolve) => setTimeout(resolve, 250))
  }
  throw new Error(`Timed out waiting for ${url}`)
}

async function isReachableBaseUrl(baseUrl) {
  try {
    const url = new URL('index.html', normalizeBaseUrl(baseUrl))
    const resp = await fetch(url, { method: 'GET' })
    return resp.ok
  } catch {
    return false
  }
}

async function ensureStaticBuild() {
  await access(path.join(DEFAULT_OUTDIR, 'index.html'))
  await access(path.join(DEFAULT_OUTDIR, 'iframe.html'))
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
      const onError = (error) => {
        server.off('error', onError)
        reject(error)
      }
      server.on('error', onError)
      server.listen(port, '127.0.0.1', () => {
        server.off('error', onError)
        resolve()
      })
    })

  const cleanup = () =>
    new Promise((resolve) => {
      for (const socket of sockets) socket.destroy()
      server.close(() => resolve())
    })

  return { cleanup, listen }
}

async function main() {
  const args = parseArgs(process.argv.slice(2))

  const repoRoot = path.resolve(process.cwd(), '..')
  const outDir = path.resolve(args.outdir ?? path.join(repoRoot, 'docs/screenshots/storybook'))
  await mkdir(outDir, { recursive: true })

  const explicitBaseUrl =
    args.url ??
    process.env.DOCKREV_STORYBOOK_URL ??
    null

  let staticServer = null
  let resolvedBaseUrl =
    explicitBaseUrl && (await isReachableBaseUrl(explicitBaseUrl))
      ? explicitBaseUrl
      : null

  if (!resolvedBaseUrl) {
    try {
      await ensureStaticBuild()
      const port = await findAvailablePort(DEFAULT_PORT)
      staticServer = startStaticServer({ port })
      await staticServer.listen()
      const localUrl = `http://127.0.0.1:${port}/`
      await waitForHttpOk(new URL('index.html', localUrl).toString())
      resolvedBaseUrl = localUrl
    } catch (error) {
      throw new Error(
        `Failed to serve local Storybook static build: ${error instanceof Error ? error.message : String(error)}`,
      )
    }
  }
  if (!resolvedBaseUrl) {
    throw new Error('Failed to resolve Storybook base URL.')
  }

  const browser = await chromium.launch()
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 2,
  })

  const openStory = async (id, viewport) => {
    const page = await context.newPage()
    page.on('dialog', (d) => d.accept().catch(() => {}))
    if (viewport) {
      await page.setViewportSize(viewport)
    }
    await page.goto(iframeUrl(resolvedBaseUrl, id), { waitUntil: 'domcontentloaded' })
    await page.waitForFunction(
      () => {
        const root = document.querySelector('#storybook-root, #root')
        return Boolean(root && root.childElementCount > 0)
      },
      null,
      { timeout: 60_000 }
    )
    await page.waitForTimeout(250)
    return page
  }

  const scrollSidebarToBottom = async (page) => {
    await page.evaluate(() => {
      const el = document.querySelector('.sidebar')
      if (!el) return
      el.scrollTop = el.scrollHeight
    })
  }

  const shots = [
    {
      id: 'layouts-appshell--overview',
      file: 'app-shell-sidebar.png',
      setup: async (page) => {
        await scrollSidebarToBottom(page)
      },
      screenshot: async (page, filePath) => {
        const el = page.locator('.sidebar')
        await el.waitFor({ timeout: STORY_TIMEOUT_MS })
        await el.screenshot({ path: filePath })
      },
    },
    {
      id: 'layouts-appshell--overview',
      file: 'app-meta-footer.png',
      setup: async (page) => {
        await scrollSidebarToBottom(page)
      },
      screenshot: async (page, filePath) => {
        const el = page.locator('.sidebarMeta')
        await el.waitFor({ timeout: STORY_TIMEOUT_MS })
        await el.screenshot({ path: filePath })
      },
    },
    {
      id: 'components-statusremark--all-statuses',
      file: 'status-remark-all-statuses.png',
      setup: async () => {},
      screenshot: async (page, filePath) => {
        const el = page.locator('.card')
        await el.waitFor({ timeout: STORY_TIMEOUT_MS })
        await el.screenshot({ path: filePath })
      },
    },
    {
      id: 'components-statusremark--all-statuses',
      file: 'status-remark-discovery-timeline-open.png',
      setup: async (page) => {
        const trigger = page.getByRole('button', { name: /发现 .*次，查看版本时间线/ }).first()
        await trigger.waitFor({ timeout: STORY_TIMEOUT_MS })
        await trigger.hover()
        await page.locator('.discoveryHistoryPopover[data-state="open"]').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.waitForTimeout(160)
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath, fullPage: true })
      },
    },
    {
      id: 'components-aggregateupdatepreviewlist--all-states',
      file: 'aggregate-update-preview-all-states.png',
      setup: async (page) => {
        await page.evaluate(() => {
          const el = document.querySelector('.modalList')
          if (!(el instanceof HTMLElement)) return
          el.style.maxHeight = 'none'
          el.style.overflow = 'visible'
          el.style.paddingRight = '0'
        })
        await page.waitForTimeout(150)
      },
      screenshot: async (page, filePath) => {
        const el = page.locator('.card')
        await el.waitFor({ timeout: STORY_TIMEOUT_MS })
        await el.screenshot({ path: filePath })
      },
    },
    {
      id: 'components-aggregateupdatepreviewlist--all-states',
      file: 'aggregate-update-preview-discovery-timeline-open.png',
      setup: async (page) => {
        const trigger = page.getByRole('button', { name: /发现 .*次，查看版本时间线/ }).first()
        await trigger.waitFor({ timeout: STORY_TIMEOUT_MS })
        await trigger.hover()
        await page.locator('.discoveryHistoryPopover[data-state="open"]').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.waitForTimeout(160)
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath, fullPage: true })
      },
    },
    {
      id: 'components-confirmdialog--demo',
      file: 'confirm-dialog-single-service.png',
      setup: async (page) => {
        const btn = page.getByRole('button', { name: '打开：服务更新' })
        await btn.waitFor({ timeout: STORY_TIMEOUT_MS })
        await btn.click()
        await page.getByText('确认更新服务').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
    },
    {
      id: 'pages-overviewpage--default',
      file: 'overview-homepage-v2-desktop.png',
      viewport: { width: 1920, height: 1000 },
      setup: async (page) => {
        await page
          .locator('.topbarGlobalContent .homepageHeaderContent')
          .waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.homepageServiceCard').first().waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.evaluate(() => window.scrollTo(0, 0))
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath, fullPage: false })
      },
    },
    {
      id: 'pages-overviewpage--dense-balanced-groups',
      file: 'overview-homepage-audit-balanced-desktop.png',
      viewport: { width: 1920, height: 1000 },
      setup: async (page) => {
        await page
          .locator('.homepageDashboardColumn')
          .first()
          .waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.homepageServiceCard').first().waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.evaluate(() => window.scrollTo(0, 0))
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath, fullPage: false })
      },
    },
    {
      id: 'pages-overviewpage--light-contrast',
      file: 'overview-homepage-audit-light-contrast.png',
      viewport: { width: 1440, height: 920 },
      setup: async (page) => {
        await page
          .locator('.homepageStatusLine')
          .waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.homepageServiceCard').first().waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.evaluate(() => window.scrollTo(0, 0))
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath, fullPage: false })
      },
    },
    {
      id: 'pages-overviewpage--mobile-stacked',
      file: 'overview-homepage-v2-mobile.png',
      viewport: { width: 390, height: 900 },
      setup: async (page) => {
        await page
          .locator('.homepageMobileNavModule .homepageTopStrip')
          .waitFor({ state: 'visible', timeout: STORY_TIMEOUT_MS })
        await page.locator('.homepageServiceCard').first().waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.evaluate(() => window.scrollTo(0, 0))
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath, fullPage: true })
      },
    },
    {
      id: 'pages-overviewpage--mobile-stacked',
      file: 'overview-homepage-v2-mobile-menu.png',
      viewport: { width: 390, height: 900 },
      setup: async (page) => {
        await page
          .locator('.homepageMobileNavModule .homepageTopStrip')
          .waitFor({ state: 'visible', timeout: STORY_TIMEOUT_MS })
        await page.getByRole('button', { name: '打开主导航' }).click()
        await page
          .locator('#mobileDockrevMenu .mobileMenuEmbeddedContent .homepageDrawerSearchSlot')
          .waitFor({ state: 'visible', timeout: STORY_TIMEOUT_MS })
        await page
          .locator('#mobileDockrevMenu .mobileMenuEmbeddedContent .homepageDrawerBottomSummary')
          .waitFor({ state: 'visible', timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const el = page.locator('#mobileDockrevMenu')
        await el.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-servicespage--dashboard-demo',
      file: 'services-dashboard.png',
      setup: async () => {},
    },
  ]

  try {
    for (const s of shots) {
      const page = await openStory(s.id, s.viewport)
      try {
        await s.setup(page)
        await page.waitForTimeout(250)
        const filePath = path.join(outDir, s.file)
        if (typeof s.screenshot === 'function') {
          await s.screenshot(page, filePath)
        } else {
          await page.screenshot({ path: filePath, fullPage: true })
        }
        console.log(`Saved: ${path.relative(repoRoot, filePath)}`)
      } finally {
        await page.close().catch(() => {})
      }
    }
  } finally {
    await browser.close().catch(() => {})
    await staticServer?.cleanup?.().catch(() => {})
  }
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
