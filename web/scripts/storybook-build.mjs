import { spawn } from 'node:child_process'

const SUCCESS_MARKER = 'Storybook build completed successfully'
const SUCCESS_GRACE_MS = 2_000
const FORCE_KILL_GRACE_MS = 2_000

function forward(stream, writer, onChunk) {
  if (!stream) return
  stream.on('data', (chunk) => {
    const text = String(chunk)
    writer.write(text)
    onChunk(text)
  })
}

async function main() {
  const passthrough = process.argv.slice(2)
  const args = [
    './node_modules/storybook/dist/bin/dispatcher.js',
    'build',
    '--disable-telemetry',
    ...passthrough,
  ]

  const child = spawn('node', args, {
    cwd: process.cwd(),
    env: {
      ...process.env,
      STORYBOOK_DISABLE_TELEMETRY: process.env.STORYBOOK_DISABLE_TELEMETRY ?? '1',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  })

  let sawSuccess = false
  let terminatedAfterSuccess = false
  let successTimer = null
  let forceKillTimer = null

  const clearTimers = () => {
    if (successTimer) {
      clearTimeout(successTimer)
      successTimer = null
    }
    if (forceKillTimer) {
      clearTimeout(forceKillTimer)
      forceKillTimer = null
    }
  }

  const scheduleExitGuard = () => {
    if (sawSuccess || successTimer) return
    sawSuccess = true
    successTimer = setTimeout(() => {
      terminatedAfterSuccess = true
      child.kill('SIGTERM')
      forceKillTimer = setTimeout(() => {
        child.kill('SIGKILL')
      }, FORCE_KILL_GRACE_MS)
      forceKillTimer.unref?.()
    }, SUCCESS_GRACE_MS)
    successTimer.unref?.()
  }

  forward(child.stdout, process.stdout, (text) => {
    if (text.includes(SUCCESS_MARKER)) scheduleExitGuard()
  })
  forward(child.stderr, process.stderr, (text) => {
    if (text.includes(SUCCESS_MARKER)) scheduleExitGuard()
  })

  child.on('error', (error) => {
    clearTimers()
    console.error(error)
    process.exit(1)
  })

  child.on('exit', (code, signal) => {
    clearTimers()
    if (sawSuccess && (code === 0 || terminatedAfterSuccess || signal === 'SIGTERM' || signal === 'SIGKILL')) {
      process.exit(0)
    }
    process.exit(code ?? 1)
  })
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
