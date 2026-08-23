import path from 'node:path'
import { access, mkdir, readFile } from 'node:fs/promises'
import http from 'node:http'
import net from 'node:net'
import { chromium } from 'playwright'

const DEFAULT_PORT = 50886
const DEFAULT_OUTDIR = path.resolve(process.cwd(), 'storybook-static')
const STORY_TIMEOUT_MS = 20_000

function parseArgs(argv) {
  const out = { url: null, outdir: null, only: [] }
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
    if (a === '--only') {
      out.only = (argv[i + 1] ?? '')
        .split(',')
        .map((item) => item.trim())
        .filter(Boolean)
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

  const fitServiceResourceEvidenceFrame = async (page) => {
    await page.locator('.serviceResourceEvidenceFrame').evaluate((frame) => {
      const card = frame.querySelector('.svcResourceCard')
      if (!(card instanceof HTMLElement)) throw new Error('Service resource card is missing.')
      frame.style.height = `${card.scrollHeight + 48}px`
    })
  }

  const shots = [
    {
      id: 'components-discoveryissuereconcileaction--eligible-warning',
      file: 'discovery-issue-reconcile-action.png',
      setup: async (page) => {
        await page.locator('[data-reconcile-action-state="eligible"]').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        await page.locator('[data-reconcile-action-state="eligible"]').screenshot({ path: filePath })
      },
    },
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
      id: 'components-serviceresourcepanel--offline-snapshot',
      file: 'service-resource-offline-snapshot.png',
      setup: async (page) => {
        await page.locator('.svcResourceCard').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const el = page.locator('.svcResourceCard')
        await el.waitFor({ timeout: STORY_TIMEOUT_MS })
        await el.screenshot({ path: filePath })
      },
    },
    {
      id: 'components-serviceresourcepanel--high-variation-curves',
      file: 'service-resource-monotone-curves.png',
      viewport: { width: 1280, height: 720 },
      setup: async (page) => {
        const chartSurface = page.locator('.svcResourceChartWrap')
        await chartSurface.waitFor({ timeout: STORY_TIMEOUT_MS })
        await chartSurface.evaluate((el) => el.scrollIntoView({ block: 'start', behavior: 'auto' }))
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath, fullPage: false })
      },
    },
    {
      id: 'components-serviceresourcepanel--high-variation-curves',
      file: 'service-resource-monotone-curves-mobile.png',
      viewport: { width: 375, height: 900 },
      setup: async (page) => {
        const chartSurface = page.locator('.svcResourceChartWrap')
        await chartSurface.waitFor({ timeout: STORY_TIMEOUT_MS })
        await chartSurface.evaluate((el) => el.scrollIntoView({ block: 'start', behavior: 'auto' }))
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath, fullPage: false })
      },
    },
    {
      id: 'components-serviceresourcepanel--window-switch-contract',
      file: 'service-resource-window-contract.png',
      viewport: { width: 1280, height: 720 },
      setup: async (page) => {
        await page.locator('.svcResourceWindowSwitch').waitFor({ timeout: STORY_TIMEOUT_MS })
        for (const label of ['3m', '1h', '24h', '7d', '30d']) {
          await page.getByRole('radio', { name: label }).waitFor({ timeout: STORY_TIMEOUT_MS })
        }
        await page.getByRole('radio', { name: '30d' }).click()
        await page.getByText('长时间窗口按时间桶展示历史均值').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.svcResourceWindowSwitch').evaluate((el) => el.scrollIntoView({ block: 'center', behavior: 'auto' }))
        await page.waitForTimeout(160)
        await fitServiceResourceEvidenceFrame(page)
      },
      screenshot: async (page, filePath) => {
        await page.locator('.serviceResourceEvidenceFrame').screenshot({ path: filePath })
      },
    },
    {
      id: 'components-serviceresourcepanel--window-switch-contract',
      file: 'service-resource-window-contract-mobile.png',
      viewport: { width: 375, height: 900 },
      setup: async (page) => {
        await page.locator('.svcResourceWindowSwitch').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.getByRole('radio', { name: '30d' }).click()
        await page.getByText('长时间窗口按时间桶展示历史均值').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.svcResourceWindowSwitch').evaluate((el) => el.scrollIntoView({ block: 'start', behavior: 'auto' }))
        await page.waitForTimeout(160)
        await fitServiceResourceEvidenceFrame(page)
      },
      screenshot: async (page, filePath) => {
        await page.locator('.serviceResourceEvidenceFrame').screenshot({ path: filePath })
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
      id: 'pages-servicedetailpage--service-protection-backup-targets',
      file: 'service-protection-backup-targets.png',
      setup: async (page) => {
        await page.getByText('Volumes').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const el = page.locator('.settingsDrawerBody').first()
        await el.waitFor({ timeout: STORY_TIMEOUT_MS })
        await el.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-servicedetailpage--settings-offline-readonly',
      file: 'service-detail-settings-offline-readonly.png',
      viewport: { width: 1440, height: 1080 },
      setup: async (page) => {
        await page.getByText('当前离线，设置页需要联网。').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const el = page.locator('.page').first()
        await el.waitFor({ timeout: STORY_TIMEOUT_MS })
        await el.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-servicedetailpage--versions-section',
      file: 'service-versions-anchor.png',
      viewport: { width: 1600, height: 1200 },
      setup: async (page) => {
        await page.locator('[data-service-detail-section-card="versions"]').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('[data-version-card-current="true"]').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.serviceVersionsScrollViewport').evaluate((el) => {
          if (!(el instanceof HTMLElement)) return
          const currentCard = el.querySelector('[data-version-card-current="true"]')
          if (!(currentCard instanceof HTMLElement)) return
          const currentRow = currentCard.closest('.serviceVersionsVirtualRow')
          const previousCard = currentRow?.previousElementSibling?.querySelector('[data-service-version-card="true"]')
          const targetCard = previousCard instanceof HTMLElement ? previousCard : currentCard
          const viewportRect = el.getBoundingClientRect()
          const cardRect = targetCard.getBoundingClientRect()
          const targetTop = el.scrollTop + (cardRect.top - viewportRect.top) - 14
          el.scrollTop = Math.max(0, targetTop)
        })
        await page.waitForTimeout(220)
      },
      screenshot: async (page, filePath) => {
        const pageSurface = page.locator('.page').first()
        await pageSurface.waitFor({ timeout: STORY_TIMEOUT_MS })
        await pageSurface.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-servicedetailpage--logs-section-light-contrast',
      file: 'service-detail-logs-light-human.png',
      viewport: { width: 1440, height: 1000 },
      setup: async (page) => {
        await page.locator('.serviceLogsTerminal').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.getByRole('button', { name: 'Human' }).click()
        await page.locator('.serviceLogHumanMsg').first().waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const pageSurface = page.locator('.page').first()
        await pageSurface.waitFor({ timeout: STORY_TIMEOUT_MS })
        await pageSurface.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-servicedetailpage--desktop-logs-timestamp-layout',
      file: 'service-detail-logs-timestamp-layout-desktop.png',
      viewport: { width: 1440, height: 1000 },
      setup: async (page) => {
        const terminal = page.locator('.serviceLogsTerminal')
        await terminal.waitFor({ state: 'attached', timeout: STORY_TIMEOUT_MS })
        await page.locator('.serviceLogTsTime').first().waitFor({ state: 'attached', timeout: STORY_TIMEOUT_MS })
        await page.locator('[aria-label="服务实时日志"]').evaluate((element) => {
          if (!(element instanceof HTMLElement)) return
          element.scrollTop = 0
        })
        const jump = page.locator('.serviceLogsJumpWrap')
        if (await jump.count()) {
          await jump.evaluate((element) => {
            if (element instanceof HTMLElement) element.style.visibility = 'hidden'
          })
        }
        await terminal.evaluate((element) => element.scrollIntoView({ block: 'start', behavior: 'auto' }))
      },
      screenshot: async (page, filePath) => {
        await page.locator('.serviceLogsTerminal').screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-servicedetailpage--mobile-logs-timestamp-layout',
      file: 'service-detail-logs-timestamp-layout-mobile.png',
      viewport: { width: 393, height: 852 },
      setup: async (page) => {
        const terminal = page.locator('.serviceLogsTerminal')
        await terminal.waitFor({ state: 'attached', timeout: STORY_TIMEOUT_MS })
        await page.locator('.serviceLogTsTime').first().waitFor({ state: 'attached', timeout: STORY_TIMEOUT_MS })
        await page.locator('[aria-label="服务实时日志"]').evaluate((element) => {
          if (!(element instanceof HTMLElement)) return
          element.scrollTop = 0
        })
        const jump = page.locator('.serviceLogsJumpWrap')
        if (await jump.count()) {
          await jump.evaluate((element) => {
            if (element instanceof HTMLElement) element.style.visibility = 'hidden'
          })
        }
        await terminal.evaluate((element) => element.scrollIntoView({ block: 'start', behavior: 'auto' }))
      },
      screenshot: async (page, filePath) => {
        await page.locator('.serviceLogsTerminal').screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-servicedetailpage--mobile-logs-section-light-contrast',
      file: 'service-detail-logs-light-raw-mobile.png',
      viewport: { width: 393, height: 852 },
      setup: async (page) => {
        await page.locator('.serviceLogsTerminal').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.getByRole('button', { name: 'Raw' }).click()
        await page.locator('.serviceLogRow[data-view="raw"]').first().waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.serviceLogsTerminal').evaluate((el) => el.scrollIntoView({ block: 'start', behavior: 'auto' }))
        await page.waitForTimeout(160)
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath, fullPage: false })
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
    {
      id: 'pages-servicespage--global-task-readable-label',
      file: 'overview-global-task-readable-label.png',
      viewport: { width: 1800, height: 960 },
      setup: async (page) => {
        await page.locator('.overviewJobsList').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.overviewJobTitle-discovery').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const dashboard = page.locator('.twoCol').first()
        await dashboard.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-queuepage--update-layer-progress',
      file: 'update-layer-progress-queue.png',
      setup: async (page) => {
        await page.locator('.page').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.getByText('已下载 4.2MB · layers 2/6').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const pageSurface = page.locator('.page').first()
        await pageSurface.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-jobdetailpage--update-layer-progress',
      file: 'update-layer-progress-job-detail.png',
      setup: async (page) => {
        await page.locator('.jobDetailPage').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.getByText('已下载 4.2MB · layers 2/6').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const pageSurface = page.locator('.jobDetailPage').first()
        await pageSurface.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-jobdetailpage--update-stop-available',
      file: 'update-stop-requested-job-detail.png',
      setup: async (page) => {
        await page.locator('.jobDetailPage').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.getByRole('button', { name: '正在停止' }).waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const pageSurface = page.locator('.jobDetailPage').first()
        await pageSurface.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-jobdetailpage--update-stop-available-evidence',
      file: 'update-stop-available-job-detail.png',
      setup: async (page) => {
        await page.locator('.jobDetailPage').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.getByRole('button', { name: '停止更新' }).waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const pageSurface = page.locator('.jobDetailPage').first()
        await pageSurface.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-jobdetailpage--update-stop-cancelled',
      file: 'update-stop-cancelled-job-detail.png',
      setup: async (page) => {
        await page.locator('.jobDetailPage').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.getByRole('button', { name: '已停止' }).waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const pageSurface = page.locator('.jobDetailPage').first()
        await pageSurface.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-jobdetailpage--long-logs-paused-follow-evidence',
      file: 'job-detail-log-follow-paused.png',
      setup: async (page) => {
        await page.locator('.jobDetailPage').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('[data-job-detail-log-surface="true"]').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.waitForFunction(
          () => Number(document.querySelector('[data-job-detail-log-surface="true"]')?.getAttribute('data-job-detail-log-count') ?? '0') >= 105,
          null,
          { timeout: STORY_TIMEOUT_MS },
        )
        await page.evaluate(() => {
          const viewport = document.querySelector('[aria-label="任务日志"]')
          if (!(viewport instanceof HTMLElement)) return
          viewport.scrollTop = Math.max(0, viewport.scrollTop - 240)
          viewport.dispatchEvent(new Event('scroll'))
        })
        await page.waitForFunction(
          () => document.querySelector('[data-job-detail-log-surface="true"]')?.getAttribute('data-job-detail-log-follow') === 'false',
          null,
          { timeout: STORY_TIMEOUT_MS },
        )
        await page.getByRole('button', { name: '跳到最新' }).waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const pageSurface = page.locator('.jobDetailPage').first()
        await pageSurface.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-settingspage--octo-rill-release-notes-card',
      file: 'octorill-settings-card.png',
      setup: async (page) => {
        const card = page.locator('.card').filter({ hasText: 'OctoRill 更新日志' }).first()
        await card.waitFor({ timeout: STORY_TIMEOUT_MS })
        await card.evaluate((el) => el.scrollIntoView({ block: 'center', behavior: 'auto' }))
        await page.waitForTimeout(160)
      },
      screenshot: async (page, filePath) => {
        const card = page.locator('.card').filter({ hasText: 'OctoRill 更新日志' }).first()
        await card.screenshot({ path: filePath })
      },
    },
    {
      id: 'components-githubreleasedrawer--octo-rill-smart-default',
      file: 'octorill-release-drawer-smart-default.png',
      setup: async (page) => {
        await page.locator('.releaseDrawerContent').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.releaseDrawerViewTabActive', { hasText: '润色' }).waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.getByText('润色摘要').first().waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        const drawer = page.locator('.releaseDrawerContent').first()
        await drawer.screenshot({ path: filePath })
      },
    },
    {
      id: 'components-githubreleasedrawer--git-hub-original-only',
      file: 'github-release-drawer-original-only.png',
      setup: async (page) => {
        await page.locator('.releaseDrawerContent').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.releaseDrawerChip', { hasText: 'GitHub Releases' }).waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.waitForFunction(
          () => document.querySelectorAll('.releaseDrawerViewTab').length === 0,
          null,
          { timeout: STORY_TIMEOUT_MS },
        )
      },
      screenshot: async (page, filePath) => {
        const drawer = page.locator('.releaseDrawerContent').first()
        await drawer.screenshot({ path: filePath })
      },
    },
    {
      id: 'components-githubreleasedrawer--anonymous-located',
      file: 'drawer-locate-found.png',
      setup: async (page) => {
        await page.locator('.releaseDrawerContent').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('[data-release-drawer-banner="success"]').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('[data-release-highlighted="true"]').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.releaseDrawerScrollViewport').evaluate((el) => {
          if (!(el instanceof HTMLElement)) return
          const highlightedCard = el.querySelector('[data-release-highlighted="true"]')
          if (!(highlightedCard instanceof HTMLElement)) return
          const viewportRect = el.getBoundingClientRect()
          const cardRect = highlightedCard.getBoundingClientRect()
          const targetTop = el.scrollTop + (cardRect.top - viewportRect.top) - 14
          el.scrollTop = Math.max(0, targetTop)
        })
        await page.waitForTimeout(220)
      },
      screenshot: async (page, filePath) => {
        const drawer = page.locator('.releaseDrawerContent').first()
        await drawer.waitFor({ timeout: STORY_TIMEOUT_MS })
        await drawer.screenshot({ path: filePath })
      },
    },
    {
      id: 'components-githubreleasedrawer--outside-window',
      file: 'drawer-outside-window.png',
      setup: async (page) => {
        await page.locator('.releaseDrawerContent').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('[data-release-drawer-banner="warning"]').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.waitForTimeout(220)
      },
      screenshot: async (page, filePath) => {
        const drawer = page.locator('.releaseDrawerContent').first()
        await drawer.waitFor({ timeout: STORY_TIMEOUT_MS })
        await drawer.screenshot({ path: filePath })
      },
    },
    {
      id: 'layouts-appshell--update-ready-bubble',
      file: 'pwa-update-bubble-desktop.png',
      setup: async (page) => {
        await page.locator('.pwaUpdateBubble').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath, fullPage: false })
      },
    },
    {
      id: 'layouts-appshell--update-ready-bubble-mobile',
      file: 'pwa-update-bubble-mobile.png',
      viewport: { width: 393, height: 852 },
      setup: async (page) => {
        await page.locator('.pwaUpdateBubble').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.mobileBottomNav').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath, fullPage: false })
      },
    },
    {
      id: 'components-readonlysnapshotnotice--offline-snapshot',
      file: 'offline-snapshot-notice.png',
      setup: async (page) => {
        await page.locator('.readonlySnapshotNotice-warn').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        await page.locator('.readonlySnapshotNotice-warn').screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-servicedetailpage--lifecycle-running-mobile',
      file: 'service-detail-mobile-actions-closed.png',
      viewport: { width: 393, height: 852 },
      setup: async (page) => {
        await page.locator('[aria-label="服务操作"]').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.keyboard.press('Escape')
        await page.locator('[role="menu"][aria-label="服务操作"]').waitFor({ state: 'hidden', timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-servicedetailpage--lifecycle-running-mobile',
      file: 'service-detail-mobile-actions-open.png',
      viewport: { width: 393, height: 852 },
      setup: async (page) => {
        await page.locator('[aria-label="服务操作"]').click()
        await page.locator('[role="menu"][aria-label="服务操作"]').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        await page.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-deploywelcomepage--default',
      file: 'deploy-check-pass-desktop.png',
      setup: async (page) => {
        await page.locator('.deployWelcomeOverall.is-pass').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.deployWelcomeActionPanel').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        await page.locator('.deployWelcomeRoot').screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-deploywelcomepage--blocked-core-failure',
      file: 'deploy-check-blocked-desktop.png',
      setup: async (page) => {
        await page.locator('.deployBlockingNotice').waitFor({ timeout: STORY_TIMEOUT_MS })
        await page.locator('.deployWelcomeActionPanel').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        await page.locator('.deployWelcomeRoot').screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-deploywelcomepage--blocked-core-failure-mobile',
      file: 'deploy-check-blocked-mobile.png',
      viewport: { width: 393, height: 852 },
      setup: async (page) => {
        await page.locator('.deployBlockingNotice').waitFor({ timeout: STORY_TIMEOUT_MS })
      },
      screenshot: async (page, filePath) => {
        await page.locator('.deployWelcomeRoot').screenshot({ path: filePath })
      },
    },
  ]

  try {
    const selectedShots =
      args.only.length > 0
        ? shots.filter((s) => args.only.includes(s.id) || args.only.includes(s.file))
        : shots
    if (selectedShots.length === 0) {
      throw new Error(`No screenshots matched --only=${args.only.join(',')}`)
    }
    for (const s of selectedShots) {
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
