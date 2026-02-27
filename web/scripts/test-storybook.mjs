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

    let baselineBulletCenterInGuide = null
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

      // Track spacing between bullets by using row-0 as baseline to avoid cross-platform subpixel offsets.
      const bulletCenterInGuide = bulletCenterY - guideBox.y
      if (baselineBulletCenterInGuide == null) baselineBulletCenterInGuide = bulletCenterInGuide
      const expected = baselineBulletCenterInGuide + ri * (rowHeight + rowGap)
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

  // 3) Queue job detail: logs should be shown on a dedicated page, and navigation must work.
  {
    const page = await openStory('pages-interactiveapp--queue-long-logs')
    try {
      const items = page.locator('.queueItem')
      await items.nth(1).waitFor({ timeout: 10_000 })

      // Open short-log job, go back, then open long-log job.
      await items.nth(0).click()
      await page.getByText('job:').waitFor({ timeout: 10_000 })
      await page.getByText('job-short').waitFor({ timeout: 10_000 })

      const back = page.getByRole('button', { name: '返回列表' })
      await back.waitFor({ timeout: 10_000 })
      await back.click()
      await page.locator('.queueList').waitFor({ timeout: 10_000 })

      await items.nth(1).click()
      await page.getByText('job:').waitFor({ timeout: 10_000 })
      await page.getByText('job-long').waitFor({ timeout: 10_000 })
      // Use an exact match so fixture expansions (more lines mentioning the digest) won't break strict mode.
      const digest = `sha256:${'9'.repeat(64)}`
      await page.getByText(digest, { exact: true }).waitFor({ timeout: 10_000 })

      const back2 = page.getByRole('button', { name: '返回列表' })
      await back2.waitFor({ timeout: 10_000 })
      await back2.click()
      await page.locator('.queueList').waitFor({ timeout: 10_000 })
    } finally {
      await page.close().catch(() => {})
    }
  }

  // 3b) Queue dual progress: split planned/completed must render as two layers on one bar.
  {
    const page = await openStory('pages-queuepage--default')
    try {
      const bar = page.locator('.queueProgressBarDual').first()
      await bar.waitFor({ timeout: 10_000 })

      const ariaValueText = await bar.getAttribute('aria-valuetext')
      if (!ariaValueText?.includes('安排 80%') || !ariaValueText.includes('完成 40%')) {
        throw new Error(`Unexpected queue dual-progress aria text: ${String(ariaValueText)}`)
      }

      const info = await bar.evaluate((el) => {
        const fills = Array.from(el.querySelectorAll('.queueProgressFill'))
        const planned = fills[0]
        const completed = fills[1]
        return {
          fillCount: fills.length,
          plannedWidth: planned ? planned.style.width : null,
          completedWidth: completed ? completed.style.width : null,
        }
      })

      if (info.fillCount < 2) throw new Error(`Expected at least 2 queue progress fill layers, got ${info.fillCount}`)
      if (info.plannedWidth === info.completedWidth) {
        throw new Error(
          `Expected queue planned/completed widths to differ for split progress, got planned=${String(info.plannedWidth)}, completed=${String(info.completedWidth)}`,
        )
      }
    } finally {
      await page.close().catch(() => {})
    }
  }

  // 3c) Queue fallback: legacy payload without planned* must fallback to planned=completed.
  {
    const page = await openStory('pages-queuepage--legacy-progress-fallback')
    try {
      const bar = page.locator('.queueProgressBarDual').first()
      await bar.waitFor({ timeout: 10_000 })

      const ariaValueText = await bar.getAttribute('aria-valuetext')
      if (!ariaValueText?.includes('安排 40%') || !ariaValueText.includes('完成 40%')) {
        throw new Error(`Unexpected queue legacy fallback aria text: ${String(ariaValueText)}`)
      }

      const info = await bar.evaluate((el) => {
        const fills = Array.from(el.querySelectorAll('.queueProgressFill'))
        const planned = fills[0]
        const completed = fills[1]
        return {
          fillCount: fills.length,
          plannedWidth: planned ? planned.style.width : null,
          completedWidth: completed ? completed.style.width : null,
        }
      })

      if (info.fillCount < 2) throw new Error(`Expected at least 2 queue progress fill layers, got ${info.fillCount}`)
      if (info.plannedWidth !== '40%' || info.completedWidth !== '40%') {
        throw new Error(
          `Expected queue legacy fallback widths to match 40%, got planned=${String(info.plannedWidth)}, completed=${String(info.completedWidth)}`,
        )
      }
    } finally {
      await page.close().catch(() => {})
    }
  }

  // 3d) Job detail dual progress: planned/completed split must be visible and accessible.
  {
    const page = await openStory('pages-jobdetailpage--running-dual-progress')
    try {
      const bar = page.locator('.jobProgressBarDual').first()
      await bar.waitFor({ timeout: 10_000 })

      const ariaValueText = await bar.getAttribute('aria-valuetext')
      if (!ariaValueText?.includes('安排 90%') || !ariaValueText.includes('完成 70%')) {
        throw new Error(`Unexpected job detail dual-progress aria text: ${String(ariaValueText)}`)
      }

      const info = await bar.evaluate((el) => {
        const fills = Array.from(el.querySelectorAll('.jobProgressFill'))
        const planned = fills[0]
        const completed = fills[1]
        return {
          fillCount: fills.length,
          plannedWidth: planned ? planned.style.width : null,
          completedWidth: completed ? completed.style.width : null,
        }
      })

      if (info.fillCount < 2) throw new Error(`Expected at least 2 job detail progress fill layers, got ${info.fillCount}`)
      if (info.plannedWidth !== '90%' || info.completedWidth !== '70%') {
        throw new Error(
          `Expected job detail split widths planned=90% completed=70%, got planned=${String(info.plannedWidth)}, completed=${String(info.completedWidth)}`,
        )
      }
    } finally {
      await page.close().catch(() => {})
    }
  }

  // 3e) Job detail fallback: legacy payload without planned* must fallback to planned=completed.
  {
    const page = await openStory('pages-jobdetailpage--legacy-progress-fallback')
    try {
      const bar = page.locator('.jobProgressBarDual').first()
      await bar.waitFor({ timeout: 10_000 })

      const ariaValueText = await bar.getAttribute('aria-valuetext')
      if (!ariaValueText?.includes('安排 40%') || !ariaValueText.includes('完成 40%')) {
        throw new Error(`Unexpected job detail legacy fallback aria text: ${String(ariaValueText)}`)
      }

      const info = await bar.evaluate((el) => {
        const fills = Array.from(el.querySelectorAll('.jobProgressFill'))
        const planned = fills[0]
        const completed = fills[1]
        return {
          fillCount: fills.length,
          plannedWidth: planned ? planned.style.width : null,
          completedWidth: completed ? completed.style.width : null,
        }
      })

      if (info.fillCount < 2) throw new Error(`Expected at least 2 job detail progress fill layers, got ${info.fillCount}`)
      if (info.plannedWidth !== '40%' || info.completedWidth !== '40%') {
        throw new Error(
          `Expected job detail legacy fallback widths to match 40%, got planned=${String(info.plannedWidth)}, completed=${String(info.completedWidth)}`,
        )
      }

      const counters = await page.locator('.jobProgressCounters').innerText()
      if (!counters.includes('安排 2/5') || !counters.includes('完成 2/5')) {
        throw new Error(`Unexpected job detail legacy fallback counters: ${counters}`)
      }
    } finally {
      await page.close().catch(() => {})
    }
  }

  // 4) Update confirm modal: version popover must be above the modal overlay (not occluded).
  {
    const page = await openStory('pages-servicespage--dashboard-demo')
    try {
      const row = page.locator('.rowLine', { hasText: 'api' }).first()
      const btn = row.getByRole('button', { name: '执行更新' })
      await btn.waitFor({ timeout: 10_000 })
      await btn.click()

      const modal = page.getByRole('dialog')
      await modal.waitFor({ timeout: 10_000 })

      const trigger = modal.locator('.versionTagsTrigger').first()
      await trigger.waitFor({ timeout: 10_000 })
      await trigger.hover()

      const popover = page.locator(".versionTagsPopover[data-state='open']")
      await popover.waitFor({ timeout: 10_000 })

      const box = await requireBoundingBox(popover, 'versionTagsPopover')
      const x = box.x + box.width / 2
      const y = box.y + box.height / 2
      const hit = await page.evaluate(
        ({ x, y }) => {
          const el = document.elementFromPoint(x, y)
          return Boolean(el && el.closest('.versionTagsPopover'))
        },
        { x, y }
      )
      if (!hit) throw new Error('Expected versionTagsPopover to be on top (not occluded by modal overlay).')
    } finally {
      await page.close().catch(() => {})
    }
  }

  // 5) Update confirm modal: no target selector; update request must be pinned to scan-time candidate digest.
  {
    const page = await openStory('pages-servicespage--dashboard-demo')
    try {
      const row = page.locator('.rowLine', { hasText: 'api' }).first()
      const btn = row.getByRole('button', { name: '执行更新' })
      await btn.waitFor({ timeout: 10_000 })
      await btn.click()

      const modal = page.getByRole('dialog')
      await modal.waitFor({ timeout: 10_000 })

      // The confirm modal should not allow selecting a target version.
      const select = modal.locator('select.select')
      if (await select.count()) throw new Error('Expected no <select> in update confirm modal (version selection removed).')

      await modal.getByRole('button', { name: '执行更新' }).click()

      await page.waitForFunction(() => Boolean(globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest), null, {
        timeout: 10_000,
      })
      const req = await page.evaluate(() => globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest ?? null)
      if (!req || typeof req !== 'object') throw new Error('No update request recorded in mock API.')

      // The dashboard demo fixture uses a deterministic digest generator: d('b','9f') => sha256: + 62 * 'b' + '9f'.
      const expectedTargetDigest = `sha256:${'b'.repeat(62)}9f`
      const targetDigest = req.targetDigest
      if (targetDigest !== expectedTargetDigest) {
        throw new Error(
          `Expected update request targetDigest=${expectedTargetDigest}, got ${String(targetDigest)} (req=${JSON.stringify(req)})`,
        )
      }
    } finally {
      await page.close().catch(() => {})
    }
  }

  // 6) Version popovers: must read scan-time digest-tags snapshot only (no live /digest-tags fan-out).
  {
    const page = await openStory('components-versiontagspopover--multi-tags')
    try {
      await page.evaluate(() => {
        if (!globalThis.__DOCKREV_MOCK_DEBUG__) return
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsSnapshotCalls = 0
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsCalls = 0
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsSnapshotUrl = null
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsUrl = null
      })

      const btn = page.getByRole('button', { name: 'v0.8.8-arm64' })
      await btn.waitFor({ timeout: 10_000 })
      await btn.click()

      const popover = page.locator(".versionTagsPopover[data-state='open']")
      await popover.waitFor({ timeout: 10_000 })

      await page.waitForFunction(() => (globalThis.__DOCKREV_MOCK_DEBUG__?.digestTagsSnapshotCalls ?? 0) > 0, null, {
        timeout: 10_000,
      })
      const dbg = await page.evaluate(() => globalThis.__DOCKREV_MOCK_DEBUG__ ?? null)
      if (!dbg) throw new Error('Missing mock debug object.')

      if (dbg.digestTagsCalls !== 0) {
        throw new Error(`Expected no /digest-tags calls, got ${dbg.digestTagsCalls} (last=${String(dbg.lastDigestTagsUrl)})`)
      }
      if (dbg.digestTagsSnapshotCalls <= 0) {
        throw new Error('Expected at least one /digest-tags-snapshot call, got 0.')
      }

      await popover.getByText('快照时间').waitFor({ timeout: 10_000 })
    } finally {
      await page.close().catch(() => {})
    }
  }

  // 7) Snapshot missing: popover should show a clear message and must not retry in a loop.
  {
    const page = await openStory('components-versiontagspopover--missing-snapshot')
    try {
      await page.evaluate(() => {
        if (!globalThis.__DOCKREV_MOCK_DEBUG__) return
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsSnapshotCalls = 0
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsCalls = 0
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsSnapshotUrl = null
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsUrl = null
      })

      const btn = page.getByRole('button', { name: 'v0.8.8-arm64' })
      await btn.waitFor({ timeout: 10_000 })
      await btn.click()

      const popover = page.locator(".versionTagsPopover[data-state='open']")
      await popover.waitFor({ timeout: 10_000 })

      await popover.getByText('快照缺失：请先执行一次 check').waitFor({ timeout: 10_000 })

      await page.waitForTimeout(700)
      const dbg = await page.evaluate(() => globalThis.__DOCKREV_MOCK_DEBUG__ ?? null)
      if (!dbg) throw new Error('Missing mock debug object.')
      if (dbg.digestTagsCalls !== 0) {
        throw new Error(`Expected no /digest-tags calls, got ${dbg.digestTagsCalls} (last=${String(dbg.lastDigestTagsUrl)})`)
      }
      if (dbg.digestTagsSnapshotCalls !== 1) {
        throw new Error(`Expected exactly one /digest-tags-snapshot call, got ${dbg.digestTagsSnapshotCalls}.`)
      }
    } finally {
      await page.close().catch(() => {})
    }
  }

  // 8) Snapshot pending: trigger text should switch to loading and recover after snapshot is ready.
  {
    const page = await openStory('components-versiontagspopover--pending-snapshot')
    try {
      await page.evaluate(() => {
        if (!globalThis.__DOCKREV_MOCK_DEBUG__) return
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsSnapshotCalls = 0
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsCalls = 0
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsSnapshotUrl = null
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsUrl = null
      })

      const trigger = page.locator('.versionLine').first().locator('.versionTagsTrigger').nth(1)
      await trigger.waitFor({ timeout: 10_000 })
      await trigger.click()

      await page.waitForFunction(() => {
        const line = document.querySelector('.versionLine')
        if (!line) return false
        const candidate = line.querySelectorAll('.versionTagsTrigger')[1]
        return candidate?.textContent?.trim() === '加载中…'
      }, null, { timeout: 10_000 })

      await page.waitForFunction(() => {
        const line = document.querySelector('.versionLine')
        if (!line) return false
        const candidate = line.querySelectorAll('.versionTagsTrigger')[1]
        return candidate?.textContent?.trim() === 'v0.8.8-arm64'
      }, null, { timeout: 10_000 })

      const popover = page.locator(".versionTagsPopover[data-state='open']")
      await popover.waitFor({ timeout: 10_000 })
      await popover.getByText('快照时间').waitFor({ timeout: 10_000 })

      const dbg = await page.evaluate(() => globalThis.__DOCKREV_MOCK_DEBUG__ ?? null)
      if (!dbg) throw new Error('Missing mock debug object.')
      if (dbg.digestTagsCalls !== 0) {
        throw new Error(`Expected no /digest-tags calls, got ${dbg.digestTagsCalls} (last=${String(dbg.lastDigestTagsUrl)})`)
      }
      if (dbg.digestTagsSnapshotCalls < 2) {
        throw new Error(`Expected at least 2 /digest-tags-snapshot calls for pending->ready, got ${dbg.digestTagsSnapshotCalls}.`)
      }
    } finally {
      await page.close().catch(() => {})
    }
  }

  // 9) Current version popover should follow the same pending->ready trigger transition.
  {
    const page = await openStory('components-currentversionpopover--pending-snapshot')
    try {
      await page.evaluate(() => {
        if (!globalThis.__DOCKREV_MOCK_DEBUG__) return
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsSnapshotCalls = 0
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsCalls = 0
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsSnapshotUrl = null
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsUrl = null
      })

      const trigger = page.locator('.versionLine').first().locator('.versionTagsTrigger').first()
      await trigger.waitFor({ timeout: 10_000 })
      await trigger.click()

      await page.waitForFunction(() => {
        const triggerEl = document.querySelector('.versionLine .versionTagsTrigger')
        return triggerEl?.textContent?.trim() === '加载中…'
      }, null, { timeout: 10_000 })

      await page.waitForFunction(() => {
        const triggerEl = document.querySelector('.versionLine .versionTagsTrigger')
        return triggerEl?.textContent?.trim() === 'v0.8.8-arm64'
      }, null, { timeout: 10_000 })

      const popover = page.locator(".versionTagsPopover[data-state='open']")
      await popover.waitFor({ timeout: 10_000 })
      await popover.getByText('快照时间').waitFor({ timeout: 10_000 })

      const dbg = await page.evaluate(() => globalThis.__DOCKREV_MOCK_DEBUG__ ?? null)
      if (!dbg) throw new Error('Missing mock debug object.')
      if (dbg.digestTagsCalls !== 0) {
        throw new Error(`Expected no /digest-tags calls, got ${dbg.digestTagsCalls} (last=${String(dbg.lastDigestTagsUrl)})`)
      }
      if (dbg.digestTagsSnapshotCalls < 2) {
        throw new Error(
          `Expected at least 2 /digest-tags-snapshot calls for current pending->ready, got ${dbg.digestTagsSnapshotCalls}.`,
        )
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
