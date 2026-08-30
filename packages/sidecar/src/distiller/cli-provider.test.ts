import { describe, expect, test } from 'bun:test'

import { buildCliInvocation } from './cli-provider'

describe('CLI Provider invocation', () => {
  test('keeps conversation content out of argv and applies safe flags', () => {
    const secret = 'private conversation'
    const cases = ['claude-code', 'codex', 'cursor', 'omp', 'pi', 'codebuddy']
    for (const provider of cases) {
      const result = buildCliInvocation(
        { provider, command: provider, model: 'model-id' },
        '/tmp/prompt.txt',
      )
      expect(result.args.join(' ')).not.toContain(secret)
      expect(result.args).toContain('model-id')
      if (provider === 'omp' || provider === 'pi') {
        expect(result.args.at(-1)).toBe('@/tmp/prompt.txt')
      }
    }
  })
})
