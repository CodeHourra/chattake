/**
 * 提炼 / Sidecar 失败时的简短引导（与 Toast 搭配，避免只显示泛化 Error）
 */

/** 是否在文案中追加 Sidecar / 配置相关提示 */
export function shouldAppendSidecarHint(message: string): boolean {
  return /sidecar|xunji-sidecar|未找到.*可执行|distill_session join/i.test(message)
}

export function shouldAppendApiHint(message: string): boolean {
  return /api|API Key|401|403|timeout|超时|网络|judge_value|distill_full/i.test(message)
}

/**
 * 在原始错误后追加一行可操作建议（仍是一段字符串，便于 Toast 展示）
 */
export function appendDistillHint(message: string): string {
  const m = message.trim()
  if (!m) return m
  if (m.includes('请确认已构建') || m.includes('请检查设置中的 API')) return m
  if (shouldAppendSidecarHint(m)) {
    return `${m} — 请确认已构建 packages/sidecar，或在「设置 → AI 配置」检查当前配置与密钥。`
  }
  if (shouldAppendApiHint(m)) {
    return `${m} — 请检查设置中的 API Key、模型、超时与网络。`
  }
  return m
}
