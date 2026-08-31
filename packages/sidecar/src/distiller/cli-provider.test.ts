import { describe, expect, test } from 'bun:test'
import { chmod, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { buildCliInvocation, CliProvider } from './cli-provider'

describe('CLI Provider invocation', () => {
  test('keeps conversation content out of argv and applies safe flags', () => {
    const secret = 'private conversation'
    const cases = ['claude-code', 'codex', 'cursor', 'grok', 'omp', 'pi', 'codebuddy']
    for (const provider of cases) {
      const result = buildCliInvocation(
        { provider, command: provider, model: 'model-id' },
        '/tmp/prompt.txt',
      )
      expect(result.args.join(' ')).not.toContain(secret)
      expect(result.args).toContain('model-id')
      if (provider === 'grok') {
        expect(result.inputMode).toBe('file')
        expect(result.args).toContain('--prompt-file')
        expect(result.args).toContain('--cwd')
        expect(result.args.at(-1)).toBe('/tmp/prompt.txt')
      } else if (provider === 'omp' || provider === 'pi') {
        expect(result.args.at(-1)).toBe('@/tmp/prompt.txt')
      }
    }
  })

  test('passes the prompt through stdin instead of argv', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'chattake-cli-test-'))
    const command = join(dir, 'fake-cli')
    await writeFile(command, '#!/bin/sh\nprintf "%s\\n" "$@" > "$(dirname "$0")/args"\ncat > "$(dirname "$0")/stdin"\nprintf ok\n')
    await chmod(command, 0o700)
    try {
      await new CliProvider({ provider: 'custom-cli', command }).distill('system', 'private conversation')
      expect(await readFile(join(dir, 'args'), 'utf8')).toBe('\n')
      expect(await readFile(join(dir, 'stdin'), 'utf8')).toContain('private conversation')
    } finally {
      await rm(dir, { recursive: true, force: true })
    }
  })

  test('reports an explicit timeout', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'chattake-cli-timeout-'))
    const command = join(dir, 'slow-cli')
    await writeFile(command, '#!/bin/sh\ncat >/dev/null\nsleep 1\n')
    await chmod(command, 0o700)
    try {
      await expect(new CliProvider({ provider: 'custom-cli', command, timeoutMs: 20 }).distill('', 'x'))
        .rejects.toThrow('请求超时')
    } finally {
      await rm(dir, { recursive: true, force: true })
    }
  })
})
