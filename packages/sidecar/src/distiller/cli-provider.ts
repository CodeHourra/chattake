import { accessSync, constants, existsSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { homedir, tmpdir } from 'node:os'
import { basename, join } from 'node:path'

import { CONTENT_HINT } from './prompts'
import type { DistillResult } from './api-provider'

export interface CliProviderConfig {
  provider: string
  command: string
  model?: string
  timeoutMs?: number
}

type InputMode = 'stdin' | 'file'
type ActiveProcess = { pid: number; exited: Promise<number>; kill(signal?: number): void }
let activeProcess: ActiveProcess | null = null

function killProcess(proc: ActiveProcess, signal: NodeJS.Signals | number): void {
  if (process.platform !== 'win32') {
    try {
      process.kill(-proc.pid, signal)
      return
    } catch {}
  }
  proc.kill(typeof signal === 'number' ? signal : signal === 'SIGKILL' ? 9 : 15)
}

process.on('SIGTERM', async () => {
  const proc = activeProcess
  if (proc) {
    let exited = false
    void proc.exited.then(() => { exited = true })
    killProcess(proc, 'SIGTERM')
    await Bun.sleep(500)
    if (!exited) killProcess(proc, 'SIGKILL')
  }
  process.exit(143)
})

export function buildCliInvocation(
  config: CliProviderConfig,
  promptPath?: string,
): { args: string[]; inputMode: InputMode } {
  const model = config.model?.trim()
  switch (config.provider) {
    case 'claude-code':
      return {
        args: ['-p', '--output-format', 'text', '--no-session-persistence', '--tools', '', '--permission-mode', 'dontAsk', ...(model ? ['--model', model] : [])],
        inputMode: 'stdin',
      }
    case 'codex':
      return {
        args: ['exec', '--ephemeral', '--skip-git-repo-check', '--ignore-rules', '--ignore-user-config', '-s', 'read-only', '--color', 'never', ...(model ? ['-m', model] : []), '-'],
        inputMode: 'stdin',
      }
    case 'cursor':
      return {
        args: ['-p', '--output-format', 'text', '--mode', 'ask', '--sandbox', 'enabled', '--trust', ...(model ? ['--model', model] : [])],
        inputMode: 'stdin',
      }
    case 'omp':
      return {
        args: ['-p', '--mode', 'text', '--no-session', '--no-tools', '--no-extensions', '--no-skills', '--no-rules', '--no-title', ...(model ? ['--model', model] : []), `@${promptPath ?? ''}`],
        inputMode: 'file',
      }
    case 'pi':
      return {
        args: ['-p', '--mode', 'text', '--no-session', '--no-tools', '--no-extensions', '--no-skills', '--no-prompt-templates', '--no-context-files', ...(model ? ['--model', model] : []), `@${promptPath ?? ''}`],
        inputMode: 'file',
      }
    case 'codebuddy':
      return {
        args: ['-p', '--output-format', 'text', '--no-session-persistence', '--tools', '', '--permission-mode', 'dontAsk', ...(model ? ['--model', model] : [])],
        inputMode: 'stdin',
      }
    default:
      return { args: [], inputMode: 'stdin' }
  }
}

function executable(command: string): string {
  const candidates = command.includes('/')
    ? [command]
    : [
        ...(process.env.PATH ?? '').split(':').filter(Boolean).map((dir) => join(dir, command)),
        join(homedir(), '.local/bin', command),
        join(homedir(), '.bun/bin', command),
        join('/opt/homebrew/bin', command),
        join('/usr/local/bin', command),
        ...(command === 'codex' ? ['/Applications/ChatGPT.app/Contents/Resources/codex'] : []),
      ]
  const found = candidates.find((path) => {
    try {
      accessSync(path, constants.X_OK)
      return true
    } catch {
      return false
    }
  })
  if (!found) throw new Error(`找不到可执行命令：${command}`)
  return found
}

async function run(
  command: string,
  args: string[],
  input: string | null,
  timeoutMs: number,
): Promise<string> {
  const proc = Bun.spawn([command, ...args], {
    detached: process.platform !== 'win32',
    stdin: input === null ? 'ignore' : 'pipe',
    stdout: 'pipe',
    stderr: 'pipe',
    env: process.env,
  })
  activeProcess = proc
  if (input !== null) {
    proc.stdin.write(input)
    proc.stdin.end()
  }
  let timedOut = false
  let killTimer: ReturnType<typeof setTimeout> | null = null
  const timer = setTimeout(() => {
    timedOut = true
    killProcess(proc, 'SIGTERM')
    killTimer = setTimeout(() => killProcess(proc, 'SIGKILL'), 500)
  }, timeoutMs)
  const [exitCode, stdout, stderr] = await Promise.all([
    proc.exited,
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]).finally(() => {
    clearTimeout(timer)
    if (killTimer) clearTimeout(killTimer)
    activeProcess = null
  })
  if (timedOut) {
    throw new Error(`${basename(command)} 请求超时（${Math.ceil(timeoutMs / 1000)} 秒）`)
  }
  if (exitCode !== 0) {
    throw new Error(`${basename(command)} 退出码 ${exitCode}：${stderr.trim().slice(0, 800) || '无错误输出'}`)
  }
  return stdout.trim()
}

export async function testCliProvider(config: CliProviderConfig): Promise<string> {
  const command = executable(config.command)
  const output = await run(command, ['--version'], null, Math.min(config.timeoutMs ?? 30_000, 30_000))
  return `命令可执行${output ? `：${output.split('\n')[0]}` : ''}`
}

export class CliProvider {
  constructor(private readonly config: CliProviderConfig) {}

  async distill(systemPrompt: string, content: string): Promise<DistillResult> {
    const command = executable(this.config.command)
    const prompt = `${systemPrompt}${CONTENT_HINT}\n\n<conversation>\n${content}\n</conversation>`
    const preview = buildCliInvocation(this.config)
    let dir: string | null = null
    try {
      let promptPath: string | undefined
      if (preview.inputMode === 'file') {
        dir = await mkdtemp(join(tmpdir(), 'chattake-cli-'))
        promptPath = join(dir, 'prompt.txt')
        await writeFile(promptPath, prompt, { encoding: 'utf8', mode: 0o600 })
      }
      const invocation = buildCliInvocation(this.config, promptPath)
      const output = await run(
        command,
        invocation.args,
        invocation.inputMode === 'stdin' ? prompt : null,
        this.config.timeoutMs ?? 120_000,
      )
      if (!output) throw new Error(`${this.config.provider} CLI 未返回内容`)
      return { content: output, promptTokens: 0, completionTokens: 0 }
    } finally {
      if (dir && existsSync(dir)) await rm(dir, { recursive: true, force: true })
    }
  }
}
