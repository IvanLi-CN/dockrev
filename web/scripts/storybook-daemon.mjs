import { spawn } from 'node:child_process'
import fs from 'node:fs'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises'

const DEFAULT_PORT = 50886
const STATE_ROOT = path.join(os.tmpdir(), 'dockrev-storybook')

let warnedMissingLsof = false
let warnedMissingPs = false

function warnMissingToolOnce(tool, error) {
  const code = error && typeof error === 'object' ? error.code : null
  const missing = code === 'ENOENT'
  if (tool === 'lsof') {
    if (warnedMissingLsof) return
    warnedMissingLsof = true
    console.error(
      `[storybook-daemon] 'lsof' ${missing ? 'not found' : 'failed'}; listener PID discovery disabled.`
    )
    return
  }
  if (tool === 'ps') {
    if (warnedMissingPs) return
    warnedMissingPs = true
    console.error(`[storybook-daemon] 'ps' ${missing ? 'not found' : 'failed'}; using HTTP checks only.`)
  }
}

function getStateDir(port) {
  return path.join(STATE_ROOT, String(port))
}

function getPidPath(port) {
  return path.join(getStateDir(port), 'pid')
}

function getLogPath(port) {
  return path.join(getStateDir(port), 'storybook.log')
}

function parsePort(value, fallback) {
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function usage() {
  console.error(
    [
      'Usage:',
      '  bun ./scripts/storybook-daemon.mjs <start|stop|status|logs> [--port <port>] [--force] [-- [extra args]]',
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
  const out = { command: null, port: null, force: false, passthrough: [] }
  const args = [...argv]
  out.command = args.shift() ?? null

  while (args.length > 0) {
    const a = args.shift()
    if (a === '--') {
      out.passthrough = args.splice(0)
      break
    }
    if (a === '--force') {
      out.force = true
      continue
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
    let settled = false
    const done = (value) => {
      if (settled) return
      settled = true
      resolve(value)
    }
    const child = spawn('lsof', ['-t', `-iTCP:${port}`, '-sTCP:LISTEN'], {
      stdio: ['ignore', 'pipe', 'ignore'],
    })

    let out = ''
    if (!child.stdout) {
      warnMissingToolOnce('lsof')
      done([])
      return
    }
    child.stdout.on('data', (chunk) => (out += String(chunk)))
    child.on('error', (error) => {
      warnMissingToolOnce('lsof', error)
      done([])
    })
    child.on('exit', () => {
      const pids = out
        .split('\n')
        .map((s) => s.trim())
        .filter(Boolean)
        .map((s) => Number(s))
        .filter((n) => Number.isFinite(n) && n > 0)
      done([...new Set(pids)])
    })
  })
}

async function getProcessCommand(pid) {
  return await new Promise((resolve) => {
    let settled = false
    const done = (value) => {
      if (settled) return
      settled = true
      resolve(value)
    }
    const child = spawn('ps', ['-p', String(pid), '-o', 'command='], {
      stdio: ['ignore', 'pipe', 'ignore'],
    })

    let out = ''
    if (!child.stdout) {
      warnMissingToolOnce('ps')
      done(null)
      return
    }
    child.stdout.on('data', (chunk) => (out += String(chunk)))
    child.on('error', (error) => {
      warnMissingToolOnce('ps', error)
      done(null)
    })
    child.on('exit', () => done(out.trim() || null))
  })
}

async function isTcpPortOpen(port, host = '127.0.0.1') {
  return await new Promise((resolve) => {
    const socket = net.connect({ port, host })

    const done = (value) => {
      socket.removeAllListeners()
      socket.destroy()
      resolve(value)
    }

    socket.once('connect', () => done(true))
    socket.once('error', () => done(false))
    socket.setTimeout(1000, () => done(false))
  })
}

async function fetchWithTimeout(url, options, timeoutMs = 2000) {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)
  try {
    return await fetch(url, { ...options, signal: controller.signal })
  } finally {
    clearTimeout(timer)
  }
}

async function isHttpOk(url) {
  try {
    const resp = await fetchWithTimeout(url, { method: 'GET' }, 2000)
    return resp.ok
  } catch {
    return false
  }
}

async function isStorybookIndexOk(port) {
  const url = `http://127.0.0.1:${port}/index.json`
  try {
    const resp = await fetchWithTimeout(url, { method: 'GET' }, 2000)
    if (!resp.ok) return false
    const json = await resp.json().catch(() => null)
    const entries = json && typeof json === 'object' ? json.entries : null
    return Boolean(entries && typeof entries === 'object')
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

async function safeReadPid(port) {
  try {
    const raw = await readFile(getPidPath(port), 'utf8')
    const pid = Number(raw.trim())
    return Number.isFinite(pid) && pid > 0 ? pid : null
  } catch {
    return null
  }
}

async function writePid(port, pid) {
  await mkdir(getStateDir(port), { recursive: true })
  await writeFile(getPidPath(port), String(pid), 'utf8')
}

async function clearPid(port) {
  await rm(getPidPath(port), { force: true })
}

function openLogFd(port) {
  fs.mkdirSync(getStateDir(port), { recursive: true })
  return fs.openSync(getLogPath(port), 'a')
}

async function cmdStatus({ port }) {
  const pids = await getListenerPids(port)
  const pidFile = await safeReadPid(port)
  const url = `http://127.0.0.1:${port}/`
  const ok = await isHttpOk(url)
  const tcpOpen = await isTcpPortOpen(port)
  const logPath = getLogPath(port)

  console.log(
    JSON.stringify(
      {
        port,
        url,
        listeningPids: pids,
        pidFile,
        httpOk: ok,
        tcpOpen,
        log: logPath,
      },
      null,
      2
    )
  )
}

async function cmdLogs({ port }) {
  const logPath = getLogPath(port)
  try {
    const content = await readFile(logPath, 'utf8')
    const lines = content.split('\n')
    const tail = lines.slice(Math.max(0, lines.length - 120)).join('\n')
    process.stdout.write(tail.endsWith('\n') ? tail : `${tail}\n`)
  } catch {
    console.error(`No logs yet at ${logPath}`)
    process.exit(1)
  }
}

async function cmdStop({ port, force }) {
  const pidFile = await safeReadPid(port)

  if (!pidFile) {
    const tcpOpen = await isTcpPortOpen(port)
    if (tcpOpen) {
      console.error(
        `Port ${port} is in use but no PID was recorded by this tool; refusing to stop it without --force.`
      )
      process.exit(1)
    }
    console.log(`No Storybook listener found on port ${port}.`)
    return
  }

  const cmd = await getProcessCommand(pidFile)
  const looksLikeStorybook =
    Boolean(cmd && cmd.includes('storybook')) || (await isStorybookIndexOk(port))
  if (!looksLikeStorybook && !force) {
    console.error(
      `Refusing to stop PID ${pidFile} (does not look like Storybook). Re-run with --force if you're sure.`
    )
    process.exit(1)
  }

  try {
    process.kill(pidFile, 'SIGTERM')
  } catch {
    await clearPid(port)
    console.log(`PID ${pidFile} is not running; cleared PID file.`)
    return
  }

  const startedAt = Date.now()
  while (Date.now() - startedAt < 10_000) {
    const tcpOpen = await isTcpPortOpen(port)
    if (!tcpOpen) break
    await new Promise((r) => setTimeout(r, 250))
  }

  if (await isTcpPortOpen(port)) {
    try {
      process.kill(pidFile, 'SIGKILL')
    } catch {}
  }

  await clearPid(port)
  console.log(`Stopped Storybook on port ${port}.`)
}

async function cmdStart({ port, passthrough }) {
  const existing = await getListenerPids(port)
  if (existing.length === 0 && (await isTcpPortOpen(port))) {
    throw new Error(`Port ${port} is already in use.`)
  }
  if (existing.length > 0) {
    throw new Error(`Port ${port} is already in use.`)
  }

  const logFd = openLogFd(port)
  const args = [
    'x',
    '--bun',
    'storybook',
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
  const spawnErrorPromise = new Promise((_, reject) => {
    child.once('error', reject)
  })
  try {
    fs.closeSync(logFd)
  } catch {}

  const url = `http://127.0.0.1:${port}/`
  try {
    await Promise.race([waitForHttpOk(url, 120_000), spawnErrorPromise])
  } catch (error) {
    try {
      process.kill(child.pid, 'SIGTERM')
    } catch {}
    await clearPid(port)
    throw error
  }

  await writePid(port, child.pid)
  child.unref()
  console.log(`Storybook ready: ${url}`)
  console.log(`Logs: ${getLogPath(port)}`)
}

async function main() {
  const { command, port: portRaw, force, passthrough } = parseArgs(process.argv.slice(2))
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
    await cmdStop({ port, force })
    return
  }
  if (command === 'status') {
    await cmdStatus({ port })
    return
  }
  if (command === 'logs') {
    await cmdLogs({ port })
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
