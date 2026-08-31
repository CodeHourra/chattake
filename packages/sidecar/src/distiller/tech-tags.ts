/**
 * 标签 / 技术栈后处理 —— 抑制「标签爆炸」、统一大小写去重。
 *
 * ── 排查指南（grep 关键词）────────────────────────────────────────
 * 1) 模型是否返回 tech_stack
 *    grep sidecar stderr: `[distiller][traceId=` 且含 `normalize: tech 相关原始值`
 *    若 tech_stack / techStack 均为 undefined → 模型未按 JSON 键输出，检查 prompt 与 JSON 解析是否丢字段。
 * 2) 归一化后是否仍为空
 *    同上日志行 `normalize: 归一化后 tech_stack`
 * 3) Rust 是否写入 DB
 *    grep 桌面日志: `insert_card:` 或 `distill_full 解析入库: tech_stack`
 * 4) 读出是否有值
 *    grep: `tech_stack列`（insert_card 日志）
 * 5) 侧栏技术栈列表
 *    调用 `list_tech_stack_counts`，若库中列为空则侧栏仍为空。
 */

const MAX_TAGS = 3
const MAX_TECH = 5

/**
 * 去重（忽略大小写）、trim、合并连续空白；保留**首次出现**的展示写法。
 */
function normalizeLabelList(raw: string[], max: number): string[] {
  const seen = new Map<string, string>()
  for (const item of raw) {
    const t = item.trim().replace(/\s+/g, ' ')
    if (!t) continue
    const key = t.toLowerCase()
    if (seen.has(key)) continue
    seen.set(key, t)
    if (seen.size >= max) break
  }
  return [...seen.values()]
}

/** 主题标签最多 3 条，避免侧栏标签爆炸。 */
export function normalizeLlmTags(raw: unknown): string[] {
  if (!Array.isArray(raw)) return []
  const strs = raw.filter((x): x is string => typeof x === 'string')
  return normalizeLabelList(strs, MAX_TAGS)
}

/** 技术项最多 5 条。 */
export function normalizeLlmTechStack(raw: unknown): string[] {
  if (!Array.isArray(raw)) return []
  const strs = raw.filter((x): x is string => typeof x === 'string')
  return normalizeLabelList(strs, MAX_TECH)
}
