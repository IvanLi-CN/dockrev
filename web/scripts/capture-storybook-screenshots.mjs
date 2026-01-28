import { spawnSync } from 'node:child_process'
import path from 'node:path'
import { mkdir } from 'node:fs/promises'
import { chromium } from 'playwright'

const DEFAULT_PORT = 50886

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

function readBaseUrlFromDaemonStatus() {
  const status = spawnSync('bun', ['./scripts/storybook-daemon.mjs', 'status', '--port', String(DEFAULT_PORT)], {
    cwd: path.resolve(process.cwd()),
    encoding: 'utf8',
  })
  if (status.status !== 0) return null
  try {
    const json = JSON.parse(String(status.stdout || '').trim())
    if (json && typeof json.url === 'string') return json.url
  } catch {
    // ignore
  }
  return null
}

async function main() {
  const args = parseArgs(process.argv.slice(2))

  const repoRoot = path.resolve(process.cwd(), '..')
  const outDir = path.resolve(args.outdir ?? path.join(repoRoot, 'docs/screenshots/storybook'))
  await mkdir(outDir, { recursive: true })

  const baseUrl =
    args.url ??
    process.env.DOCKREV_STORYBOOK_URL ??
    readBaseUrlFromDaemonStatus() ??
    `http://127.0.0.1:${DEFAULT_PORT}/`

  const browser = await chromium.launch()
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 2,
  })

  const openStory = async (id) => {
    const page = await context.newPage()
    page.on('dialog', (d) => d.accept().catch(() => {}))
    await page.goto(iframeUrl(baseUrl, id), { waitUntil: 'domcontentloaded' })
    await page.waitForFunction(() => document.body.classList.contains('sb-show-main'), null, { timeout: 60_000 })
    return page
  }

  const shots = [
    {
      id: 'components-statusremark--all-statuses',
      file: 'status-remark-all-statuses.png',
      setup: async () => {},
    },
    {
      id: 'components-confirmdialog--demo',
      file: 'confirm-dialog-single-service.png',
      setup: async (page) => {
        const btn = page.getByRole('button', { name: '打开：服务更新' })
        await btn.waitFor({ timeout: 10_000 })
        await btn.click()
        await page.getByText('确认更新服务').waitFor({ timeout: 10_000 })
      },
    },
    {
      id: 'pages-overviewpage--default',
      file: 'overview-default-confirm.png',
      setup: async (page) => {
        const btn = page.getByRole('button', { name: '更新全部' })
        await btn.waitFor({ timeout: 10_000 })
        await page.waitForFunction(
          () => {
            const el = Array.from(document.querySelectorAll('button')).find((b) => b.textContent?.trim() === '更新全部')
            return Boolean(el && !el.disabled)
          },
          null,
          { timeout: 15_000 }
        )
        await btn.click()
        await page.getByText(/确认更新.*服务/).waitFor({ timeout: 10_000 })
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
      const page = await openStory(s.id)
      try {
        await s.setup(page)
        await page.waitForTimeout(250)
        await page.screenshot({ path: path.join(outDir, s.file), fullPage: true })
        console.log(`Saved: ${path.relative(repoRoot, path.join(outDir, s.file))}`)
      } finally {
        await page.close().catch(() => {})
      }
    }
  } finally {
    await browser.close().catch(() => {})
  }
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
