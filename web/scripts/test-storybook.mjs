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

function approxEqual(a, b, tolerancePx = 1) {
  return Math.abs(a - b) <= tolerancePx
}

async function requireBoundingBox(locator, label) {
  const box = await locator.boundingBox()
  if (!box) throw new Error(`Missing bounding box: ${label}`)
  return box
}

async function assertGroupGuideAligned(page, label) {
  const allGroups = page.locator('.tableGroup')
  await allGroups.first().waitFor({ timeout: 10_000 })

  const groups = page.locator('.tableGroupExpanded')
  let groupCount = await groups.count()
  if (groupCount === 0) {
    // Story state may render groups collapsed (or render delay); try expanding the first group.
    const head = allGroups.first().locator('.groupHead')
    await head.click({ timeout: 10_000 })
    await groups.first().waitFor({ timeout: 10_000 })
    groupCount = await groups.count()
  }
  if (groupCount === 0) throw new Error(`No expanded table groups found${label ? ` (${label})` : ''}.`)

  for (let gi = 0; gi < groupCount; gi += 1) {
    const group = groups.nth(gi)
    const guide = group.locator('.groupGuide')
    const rows = group.locator('.rowLine')

    await guide.waitFor({ timeout: 10_000 })
    const rowCount = await rows.count()
    if (rowCount === 0) continue

    const guideBox = await requireBoundingBox(guide, `groupGuide[${gi}]`)
    const row0Box = await requireBoundingBox(rows.nth(0), `rowLine[${gi}][0]`)

    // The guide's top should start exactly at the first row top.
    if (!approxEqual(guideBox.y, row0Box.y, 1)) {
      throw new Error(
        `Guide top misaligned (group=${gi}${label ? `, ${label}` : ''}): guide.y=${guideBox.y}, row0.y=${row0Box.y}`
      )
    }

    const rowHeight = row0Box.height
    let rowGap = 0
    if (rowCount > 1) {
      const row1Box = await requireBoundingBox(rows.nth(1), `rowLine[${gi}][1]`)
      rowGap = row1Box.y - (row0Box.y + row0Box.height)
      // Flex `gap` should never be negative; tolerate minor rounding.
      if (rowGap < -0.5) {
        throw new Error(
          `Row gap is negative (group=${gi}${label ? `, ${label}` : ''}): gap=${rowGap}, row0.height=${row0Box.height}`
        )
      }
    }

    for (let ri = 0; ri < rowCount; ri += 1) {
      const rowBox = await requireBoundingBox(rows.nth(ri), `rowLine[${gi}][${ri}]`)
      if (!approxEqual(rowBox.height, rowHeight, 1)) {
        throw new Error(
          `Row height drift (group=${gi}, row=${ri}${label ? `, ${label}` : ''}): row.height=${rowBox.height}, expected~${rowHeight}`
        )
      }

      const bullet = rows.nth(ri).locator('.svcBullet')
      const bulletBox = await requireBoundingBox(bullet, `svcBullet[${gi}][${ri}]`)
      const bulletCenterY = bulletBox.y + bulletBox.height / 2
      const bulletCenterX = bulletBox.x + bulletBox.width / 2

      // Bullet is centered in the row by CSS (`top: 50%`).
      const bulletCenterInRow = bulletCenterY - rowBox.y
      if (!approxEqual(bulletCenterInRow, rowHeight / 2, 1)) {
        throw new Error(
          `Bullet not vertically centered (group=${gi}, row=${ri}${label ? `, ${label}` : ''}): centerInRow=${bulletCenterInRow}, expected~${rowHeight / 2}`
        )
      }

      // Bullet should also be horizontally centered on the guide line.
      const guideCenterX = guideBox.x + guideBox.width / 2
      if (!approxEqual(bulletCenterX, guideCenterX, 1)) {
        throw new Error(
          `Bullet-guide X misaligned (group=${gi}, row=${ri}${label ? `, ${label}` : ''}): bullet.centerX=${bulletCenterX}, guide.centerX=${guideCenterX}`
        )
      }

      // Bullet center should land at the midpoint of each row segment when measured from guide top.
      const bulletCenterInGuide = bulletCenterY - guideBox.y
      const expected = rowHeight / 2 + ri * (rowHeight + rowGap)
      if (!approxEqual(bulletCenterInGuide, expected, 1)) {
        throw new Error(
          `Bullet-guide alignment drift (group=${gi}, row=${ri}${label ? `, ${label}` : ''}): actual=${bulletCenterInGuide}, expected~${expected}`
        )
      }
    }
  }
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

  // 0) Group guide line alignment must remain stable (no JS measuring).
  {
    const storyIds = ['pages-overviewpage--guide-line-long-names', 'pages-servicespage--guide-line-long-names']
    for (const id of storyIds) {
      const page = await openStory(id)
      try {
        await assertGroupGuideAligned(page, id)

        const row0Before = await requireBoundingBox(page.locator('.tableGroupExpanded .rowLine').first(), `${id}:row0`)
        await page.addStyleTag({
          content: `.tableGroup { --dockrev-table-font-size: 14px; --dockrev-table-line-height: 1.7; }`,
        })
        await page.waitForTimeout(100)
        const row0After = await requireBoundingBox(page.locator('.tableGroupExpanded .rowLine').first(), `${id}:row0`)

        if (!(row0After.height > row0Before.height + 0.5)) {
          throw new Error(
            `Expected row height to scale with font changes (${id}): before=${row0Before.height}, after=${row0After.height}`
          )
        }

        await assertGroupGuideAligned(page, `${id} (scaled)`)
      } finally {
        await page.close().catch(() => {})
      }
    }
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

  // 3) Queue layout stability: switching to a job with long log lines must not squeeze the left column.
  {
    const page = await openStory('pages-queuepage--long-logs')
    try {
      const items = page.locator('.queueItem')
      await items.nth(1).waitFor({ timeout: 10_000 })

      // Select the short-log job first, then the long-log job (repro sequence from production).
      await items.nth(0).click()
      await items.nth(1).click()
      await page.getByText('sha256:9999999999').waitFor({ timeout: 10_000 })

      const cards = page.locator('.page.twoCol > .card')
      const left = await requireBoundingBox(cards.nth(0), 'queue:leftCard')
      const right = await requireBoundingBox(cards.nth(1), 'queue:rightCard')

      const ratio = left.width / (left.width + right.width)
      if (!(ratio > 0.4 && ratio < 0.6)) {
        throw new Error(`Queue columns are imbalanced after long logs: ratio=${ratio} (left=${left.width}, right=${right.width})`)
      }
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
