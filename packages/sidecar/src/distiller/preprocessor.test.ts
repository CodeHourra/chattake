import { describe, expect, test } from 'bun:test'
import { preprocess } from './preprocessor'

describe('preprocess', () => {
  test('keeps edges, samples middle turns, and respects 24k budget', () => {
    const turns = Array.from({ length: 20 }, (_, index) =>
      `[${index % 2 ? 'assistant' : 'user'}]\nTURN-${index} ${'x'.repeat(1800)}`,
    )
    const result = preprocess(turns.join('\n\n'))
    expect(result.length).toBeLessThanOrEqual(24_000)
    expect(result).toContain('TURN-0')
    expect(result).toContain('TURN-19')
    expect(result).toContain('完整轮次均匀抽样')
    expect(result).toMatch(/TURN-(?:[2-9]|1[0-7])/)
  })
})
