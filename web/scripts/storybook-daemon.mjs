import { spawn } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises'

const DEFAULT_PORT = 50886
const STATE_DIR = path.join(os.tmpdir(), 'dockrev-storybook')
const PID_PATH = path.join(STATE_DIR, 'pid')
const LOG_PATH = path.join(STATE_DIR, 'storybook.log')

function parsePort(value, fallback) {
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function usage() {
  console.error(
    [
      'Usage:',
      '  bun ./scripts/storybook-daemon.mjs <start|stop|status|logs> [--port <port>] [-- [extra args]]',
      '',
      'Examples:',
      '  bun run storybook:status',
      '  bun run storybook:start',
      '  bun run storybook:stop',
      '  bun run storybook:logs',
      '  DOCKREV_STORYBOOK_PORT=6006 bun run storybook:start -- --host 0.0.0.0',
    ].join('\n')
  )
}

function parseArgs(argv) {
  const out = { command: null, port: null, passthrough: [] }
  const args = [...argv]
  out.command = args.shift() ?? null

  while (args.length > 0) {
    const a = args.shift()
    if (a === '--') {
      out.passthrough = args.splice(0)
      break
    }
    if (a === '--port') {
      out.port = args.shift() ?? null
      continue
    }
    if (a && a.startsWith('--port=')) {
      out.port = a.slice('--port='.length)
      continue
    }
    out.passthrough.push(a)
  }

  return out
}

async function getListenerPids(port) {
  return await new Promise((resolve) => {
    const child = spawn('lsof', ['-t', `-iTCP:${port}`, '-sTCP:LISTEN'], {
      stdio: ['ignore', 'pipe', 'ignore'],
    })

    let out = ''
    child.stdout.on('data', (chunk) => {
      out += String(chunk)
    })
    child.on('exit', () => {
      const pids = out
        .split('\n')
        .map((s) => s.trim())
        .filter(Boolean)
        .map((s) => Number(s))
        .filter((n) => Number.isFinite(n) && n > 0)
      resolve([...new Set(pids)])
    })
  })
}

async function isHttpOk(url) {
  try {
    const resp = await fetch(url, { method: 'GET' })
    return resp.ok
  } catch {
    return false
  }
}

async function waitForHttpOk(url, timeoutMs = 60_000) {
  const startedAt = Date.now()
  while (Date.now() - startedAt < timeoutMs) {
    if (await isHttpOk(url)) return
    await new Promise((r) => setTimeout(r, 250))
  }
  throw new Error(`Timed out waiting for ${url}`)
}

async function safeReadPid() {
  try {
    const raw = await readFile(PID_PATH, 'utf8')
    const pid = Number(raw.trim())
    return Number.isFinite(pid) && pid > 0 ? pid : null
  } catch {
    return null
  }
}

async function writePid(pid) {
  await mkdir(STATE_DIR, { recursive: true })
  await writeFile(PID_PATH, String(pid), 'utf8')
}

async function clearPid() {
  await rm(PID_PATH, { force: true })
}

function openLogFd() {
  fs.mkdirSync(STATE_DIR, { recursive: true })
  return fs.openSync(LOG_PATH, 'a')
}

async function cmdStatus({ port }) {
  const pids = await getListenerPids(port)
  const pidFile = await safeReadPid()
  const url = `http://127.0.0.1:${port}/`
  const ok = await isHttpOk(url)

  console.log(
    JSON.stringify(
      {
        port,
        url,
        listeningPids: pids,
        pidFile,
        httpOk: ok,
        log: LOG_PATH,
      },
      null,
      2
    )
  )
}

async function cmdLogs() {
  try {
    const content = await readFile(LOG_PATH, 'utf8')
    const lines = content.split('\n')
    const tail = lines.slice(Math.max(0, lines.length - 120)).join('\n')
    process.stdout.write(tail.endsWith('\n') ? tail : `${tail}\n`)
  } catch {
    console.error(`No logs yet at ${LOG_PATH}`)
    process.exit(1)
  }
}

async function cmdStop({ port }) {
  const pids = await getListenerPids(port)
  if (pids.length === 0) {
    await clearPid()
    console.log(`No Storybook listener found on port ${port}.`)
    return
  }

  for (const pid of pids) {
    try {
      process.kill(pid, 'SIGTERM')
    } catch {}
  }

  const startedAt = Date.now()
  while (Date.now() - startedAt < 10_000) {
    const remaining = await getListenerPids(port)
    if (remaining.length === 0) break
    await new Promise((r) => setTimeout(r, 250))
  }

  const remaining = await getListenerPids(port)
  if (remaining.length > 0) {
    for (const pid of remaining) {
      try {
        process.kill(pid, 'SIGKILL')
      } catch {}
    }
  }

  await clearPid()
  console.log(`Stopped Storybook on port ${port}.`)
}

async function cmdStart({ port, passthrough }) {
  const existing = await getListenerPids(port)
  if (existing.length > 0) {
    console.log(`Storybook is already listening on port ${port}.`)
    return
  }

  const logFd = openLogFd()
  const args = [
    './node_modules/.bin/storybook',
    'dev',
    '--port',
    String(port),
    '--exact-port',
    '--no-open',
    '--debug',
    ...passthrough,
  ]

  const child = spawn('bun', args, {
    detached: true,
    stdio: ['ignore', logFd, logFd],
    env: {
      ...process.env,
      DOCKREV_STORYBOOK_PORT: String(port),
    },
  })

  child.unref()
  await writePid(child.pid)

  const url = `http://127.0.0.1:${port}/`
  await waitForHttpOk(url, 120_000)
  console.log(`Storybook ready: ${url}`)
  console.log(`Logs: ${LOG_PATH}`)
}

async function main() {
  const { command, port: portRaw, passthrough } = parseArgs(process.argv.slice(2))
  if (!command || command === '-h' || command === '--help') {
    usage()
    process.exit(command ? 0 : 2)
  }

  const port = parsePort(portRaw ?? process.env.DOCKREV_STORYBOOK_PORT, DEFAULT_PORT)

  if (command === 'start') {
    await cmdStart({ port, passthrough })
    return
  }
  if (command === 'stop') {
    await cmdStop({ port })
    return
  }
  if (command === 'status') {
    await cmdStatus({ port })
    return
  }
  if (command === 'logs') {
    await cmdLogs()
    return
  }

  console.error(`Unknown command: ${command}`)
  usage()
  process.exit(2)
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})

