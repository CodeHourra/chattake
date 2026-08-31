/**
 * 知识卡片 `type` 字段：存储值为英文枚举，UI 统一通过本模块转为中文。
 *
 * 与 v0.2 知识提取协议的固定类型一致。
 */

/** 英文 type → 中文展示名（单一数据源） */
export const CARD_TYPE_LABELS = {
  decision: '决策',
  troubleshooting: '排障',
  implementation: '实现',
  explanation: '解释',
  snippet: '片段',
} as const

/** 与 `CARD_TYPE_LABELS` 键集合一致，供类型标注使用 */
export type CardType = keyof typeof CARD_TYPE_LABELS

/**
 * 将存储用的 type 转为界面展示文案；未知键原样返回，避免空白。
 */
export function getCardTypeLabel(code: string | null | undefined): string {
  if (code == null || code === '') return ''
  const map = CARD_TYPE_LABELS as Record<string, string>
  return map[code] ?? code
}
