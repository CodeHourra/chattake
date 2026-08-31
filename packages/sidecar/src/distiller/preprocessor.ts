/** 清理内部标签，并在 24k 字符预算内保留完整首尾轮次与均匀中段样本。 */

/** 需要从对话中剥离的 XML 标签模式 */
const STRIP_PATTERNS = [
  /<thinking>[\s\S]*?<\/thinking>/g,
  /<antml_function_calls>[\s\S]*?<\/antml_function_calls>/g,
  /<function_calls>[\s\S]*?<\/function_calls>/g,
  /<tool_use>[\s\S]*?<\/tool_use>/g,
  /<tool_call>[\s\S]*?<\/tool_call>/g,
  /<tool_result>[\s\S]*?<\/tool_result>/g,
  /<antml_thinking>[\s\S]*?<\/antml_thinking>/g,
]

export function clean(content: string): string {
  let result = content
  for (const pattern of STRIP_PATTERNS) result = result.replace(pattern, '')
  return result.replace(/\n{3,}/g, '\n\n').trim()
}

function fitEdge(block: string, budget: number): string {
  if (block.length <= budget) return block
  const marker = '\n[… 单轮内容超出预算 …]\n'
  const keep = Math.max(0, budget - marker.length)
  const head = Math.floor(keep / 2)
  return `${block.slice(0, head)}${marker}${block.slice(-(keep - head))}`
}

export function truncate(content: string, maxChars = 24_000): string {
  if (content.length <= maxChars) return content

  const blocks = content.split(/\n\n(?=\[(?:user|assistant|model)\]\n)/i)
  if (blocks.length < 2) return fitEdge(content, maxChars)

  const marker = '\n\n[… 已按完整轮次均匀抽样 …]\n\n'
  const edgeBudget = Math.floor((maxChars - marker.length) * 0.36)
  const first = fitEdge(blocks[0], edgeBudget)
  const last = fitEdge(blocks.at(-1)!, edgeBudget)
  const middle = blocks.slice(1, -1)
  let budget = maxChars - first.length - last.length - marker.length
  const picked: string[] = []

  if (budget > 0 && middle.length) {
    const average = middle.reduce((sum, block) => sum + block.length + 2, 0) / middle.length
    const count = Math.max(1, Math.min(middle.length, Math.floor(budget / average)))
    const indexes = new Set<number>()
    for (let i = 0; i < count; i++) {
      indexes.add(Math.round(((i + 1) * (middle.length + 1)) / (count + 1)) - 1)
    }
    for (const index of [...indexes].sort((a, b) => a - b)) {
      const block = middle[index]
      if (block.length + 2 <= budget) {
        picked.push(block)
        budget -= block.length + 2
      }
    }
  }

  return [first, marker.trim(), ...picked, last].join('\n\n').slice(0, maxChars)
}

export function preprocess(content: string, maxChars = 24_000): string {
  return truncate(clean(content), maxChars)
}
