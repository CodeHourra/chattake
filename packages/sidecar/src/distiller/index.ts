/**
 * Distiller 模块入口 —— 组装前处理器 + OpenAI-compatible API + Prompt 模板，
 * 导出 JSON-RPC handler 供 index.ts 注册使用。
 *
 * v0.2 仅支持 HTTP API；不再保留 CLI Provider。
 */

import { createHash } from 'node:crypto'

import { clean, truncate } from './preprocessor'
import { PROMPT_B_LIGHT, PROMPT_B_FULL } from './prompts'
import { ApiProvider, listApiModels, type ApiProviderConfig, type DistillResult } from './api-provider'
import { normalizeLlmTags, normalizeLlmTechStack } from './tech-tags'
import { distillLog, resolveTraceId } from './trace'

/** 提炼调用需要的最小 Provider 契约。 */
interface Provider {
  distill(
    systemPrompt: string,
    content: string,
    callLabel?: string,
    traceId?: string,
  ): Promise<DistillResult>
}

let provider: Provider | null = null

/** LLM 提炼结果类型定义（完整版） */
export interface DistillFullResult {
  title: string
  type: string
  value: string
  value_reason?: string
  summary: string
  note: string
  tags: string[]
  tech_stack: string[]
  prompt_tokens: number
  completion_tokens: number
}

/** 价值判断结果类型定义（轻量版） */
export interface JudgeValueResult {
  value: string
  type: string
  reason: string
  prompt_tokens: number
  completion_tokens: number
}

/**
 * 初始化 API Provider（mode = "api"）。
 */
export function initApiProvider(config: ApiProviderConfig): void {
  provider = new ApiProvider(config)
  console.error(`[distiller] API Provider initialized: ${config.provider}/${config.model}`)
}

function getProvider(): Provider {
  if (!provider) {
    throw new Error('Distiller 未初始化，请先调用 init 方法配置 API')
  }
  return provider
}

/**
 * 对对话内容做前处理（清理 + 截断）
 */
function preprocess(content: string): string {
  const cleaned = clean(content)
  return truncate(cleaned)
}

/**
 * 对送入模型的正文做指纹，用于日志中对照「同一会话、同一预处理结果」是否一致。
 * （不记录全文，避免日志泄露完整对话。）
 */
function contentFingerprint(content: string): string {
  return createHash('sha256').update(content, 'utf8').digest('hex').slice(0, 16)
}

/**
 * 摘要打印正文头尾，确认截断策略是否生效、是否误伤关键信息。
 */
function logContentPreview(
  label: string,
  rawLen: number,
  processed: string,
  traceId: string,
): void {
  const fp = contentFingerprint(processed)
  const head = processed.slice(0, 160).replace(/\n/g, '↵')
  const tail = processed.slice(-160).replace(/\n/g, '↵')
  console.error(
    distillLog(
      traceId,
      `${label}: 原文=${rawLen} 字符 → 预处理后=${processed.length} 字符, sha256[0:16]=${fp}`,
    ),
  )
  console.error(distillLog(traceId, `${label}: 预处理后开头160: ${head}`))
  console.error(distillLog(traceId, `${label}: 预处理后结尾160: ${tail}`))
}

/**
 * JSON.parse 失败时输出可操作的诊断信息（位置、片段、启发式原因）。
 */
function logJsonParseDiagnostics(
  stage: string,
  cleaned: string,
  err: unknown,
  traceId: string,
): void {
  const msg = err instanceof Error ? err.message : String(err)
  const posMatch = msg.match(/position\s+(\d+)/i)
  const pos = posMatch ? Number(posMatch[1]) : NaN
  console.error(distillLog(traceId, `${stage}: JSON.parse 错误: ${msg}`))
  if (!Number.isNaN(pos) && pos >= 0 && pos < cleaned.length) {
    const from = Math.max(0, pos - 80)
    const to = Math.min(cleaned.length, pos + 120)
    const ctx = cleaned.slice(from, to).replace(/\n/g, '↵')
    console.error(
      distillLog(traceId, `${stage}: 错误位置≈${pos}, 上下文[${from},${to}]: ${ctx}`),
    )
    console.error(
      distillLog(
        traceId,
        `${stage}: 错误位置字符: ${JSON.stringify(cleaned.slice(pos, pos + 1))}`,
      ),
    )
  }
  const noteKey = '"note"'
  const ni = cleaned.indexOf(noteKey)
  if (ni !== -1) {
    const afterNote = cleaned.slice(ni, ni + 900).replace(/\n/g, '↵')
    console.error(distillLog(traceId, `${stage}: 自 ${noteKey} 起 900 字符: ${afterNote}`))
  } else {
    console.error(
      distillLog(
        traceId,
        `${stage}: 响应中未找到 ${noteKey} 键（可能键名异常或仅为代码块）`,
      ),
    )
  }
}

/**
 * 从 LLM 响应文本中提取 JSON。
 *
 * 兼容 LLM 常见的输出格式问题：
 * 1. markdown 代码块包裹（```json / ```bash / ```text / ``` 等任意语言标记）
 * 2. JSON 前后有多余文字
 * 3. 字符串值内含未转义的换行符
 * 4. 控制字符污染
 *
 * @param text - LLM 返回的原始响应文本
 * @param traceId - 提炼 trace，串联日志
 */
function extractJson(text: string, traceId: string): unknown {
  const rawPreview = text.slice(0, 300).replace(/\n/g, '↵')
  const respLen = text.length
  const tripleBackticks = (text.match(/```/g) ?? []).length
  console.error(
    distillLog(
      traceId,
      `extractJson: 响应长度=${respLen}, 三反引号出现次数=${tripleBackticks}（note 内嵌套代码块时易破坏 JSON）`,
    ),
  )
  console.error(distillLog(traceId, `extractJson 原始响应（前300字符）: ${rawPreview}`))

  let cleaned = text.trim()

  // 策略 0：移除 markdown 代码块
  // 匹配任意语言标记（json / bash / text / 空等），修复原来只匹配 json 导致 ```bash 漏处理
  const codeBlockMatch = cleaned.match(/^```[a-zA-Z0-9_-]*\s*\n?([\s\S]*?)\n?\s*```\s*$/)
  if (codeBlockMatch) {
    cleaned = codeBlockMatch[1].trim()
    console.error(
      distillLog(traceId, `已剥离最外层代码块，清理后长度=${cleaned.length}`),
    )
    console.error(
      distillLog(
        traceId,
        `已剥离代码块，清理后前200字符: ${cleaned.slice(0, 200).replace(/\n/g, '↵')}`,
      ),
    )
  } else {
    console.error(
      distillLog(traceId, `未匹配整段 \`\`\` 包裹（策略0跳过），将按原文解析`),
    )
  }

  // 策略 1：直接解析（最理想情况）
  try {
    return JSON.parse(cleaned)
  } catch (e1) {
    console.error(
      distillLog(traceId, `策略1（直接解析）失败: ${(e1 as Error).message}`),
    )
    logJsonParseDiagnostics('策略1', cleaned, e1, traceId)
  }

  // 策略 2：提取最外层 { ... }（LLM 在 JSON 前后附加了解释文字）
  const braceMatch = cleaned.match(/\{[\s\S]*\}/)
  if (braceMatch) {
    try {
      return JSON.parse(braceMatch[0])
    } catch (e2) {
      console.error(
        distillLog(traceId, `策略2（提取 {} 块）失败: ${(e2 as Error).message}`),
      )
      logJsonParseDiagnostics('策略2', braceMatch[0], e2, traceId)
    }
  }

  // 策略 3：处理 note/summary 等字段中未转义的换行符和特殊字符
  const fixedNewlines = (braceMatch?.[0] ?? cleaned).replace(
    /("(?:note|summary|reason|value_reason|title)":\s*")([\s\S]*?)("(?:\s*[,}]))/g,
    (_match, prefix: string, value: string, suffix: string) => {
      const escaped = value
        .replace(/\\/g, '\\\\')
        .replace(/\n/g, '\\n')
        .replace(/\r/g, '\\r')
        .replace(/\t/g, '\\t')
        .replace(/"/g, '\\"')
      const deduped = escaped
        .replace(/\\\\\\\\n/g, '\\\\n')
        .replace(/\\\\\\\\"/g, '\\\\"')
      return `${prefix}${deduped}${suffix}`
    },
  )

  try {
    return JSON.parse(fixedNewlines)
  } catch (e3) {
    console.error(
      distillLog(traceId, `策略3（修复换行符）失败: ${(e3 as Error).message}`),
    )
    logJsonParseDiagnostics('策略3', fixedNewlines, e3, traceId)
  }

  // 策略 4：去除控制字符后再解析
  const sanitized = (braceMatch?.[0] ?? cleaned)
    .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f]/g, '')
  try {
    return JSON.parse(sanitized)
  } catch (e4) {
    console.error(
      distillLog(traceId, `策略4（去控制字符）失败: ${(e4 as Error).message}`),
    )
    logJsonParseDiagnostics('策略4', sanitized, e4, traceId)
  }

  // 所有策略失败——记录完整响应供排查
  console.error(distillLog(traceId, `JSON 解析全部失败，完整响应:\n${text}`))
  throw new Error(
    `JSON 解析失败，已尝试4种策略。响应预览（前500字符）: ${text.slice(0, 500)}`,
  )
}

/**
 * 将 LLM 返回的「完整提炼」JSON 规范化为 DistillFullResult 所需字段。
 *
 * 说明：不少模型会输出 camelCase（如 techStack），而协议与 Rust 侧约定为 snake_case（tech_stack）。
 * 若不做归一化，Rust 反序列化时 tech_stack 会落默认空数组，前端笔记头就永远没有技术栈。
 */
function normalizeDistillFullParsed(
  raw: Record<string, unknown>,
  traceId: string,
): {
  title: string
  type: string
  value: string
  value_reason?: string
  summary: string
  note: string
  tags: string[]
  tech_stack: string[]
} {
  const tags = normalizeLlmTags(raw.tags)
  // tech_stack / techStack 二选一（及少数模型用大写键的兜底）
  const tech_stack = normalizeLlmTechStack(
    raw.tech_stack ?? raw.techStack ?? raw['TechStack'],
  )

  console.error(
    distillLog(
      traceId,
      `normalize: LLM JSON 顶层键: ${Object.keys(raw).join(', ')}`,
    ),
  )
  console.error(
    distillLog(
      traceId,
      `normalize: tech 相关原始值 tech_stack=${JSON.stringify(raw.tech_stack)} techStack=${JSON.stringify(raw.techStack)} TechStack=${JSON.stringify(raw['TechStack'])}`,
    ),
  )
  console.error(
    distillLog(
      traceId,
      `normalize: 标签 归一化后共 ${tags.length} 项（已限 5、去重）: ${JSON.stringify(tags)}`,
    ),
  )
  console.error(
    distillLog(
      traceId,
      `normalize: tech_stack 归一化后共 ${tech_stack.length} 项（已限 8、去重）: ${JSON.stringify(tech_stack)}`,
    ),
  )

  return {
    title: typeof raw.title === 'string' ? raw.title : '',
    type: typeof raw.type === 'string' ? raw.type : 'other',
    value: typeof raw.value === 'string' ? raw.value : 'medium',
    value_reason:
      typeof raw.value_reason === 'string' ? raw.value_reason : undefined,
    summary: typeof raw.summary === 'string' ? raw.summary : '',
    note: typeof raw.note === 'string' ? raw.note : '',
    tags,
    tech_stack,
  }
}

// ── JSON-RPC Handlers ──

/**
 * judge_value handler —— 轻量价值判断。
 * 输入：{ content: string }
 * 输出：{ value, type, reason, prompt_tokens, completion_tokens }
 */
export async function handleJudgeValue(
  params: Record<string, unknown>,
): Promise<JudgeValueResult> {
  if (typeof params.content !== 'string' || !params.content.trim()) {
    throw new Error('参数缺失或类型错误: content 必须为非空字符串')
  }
  const traceId = resolveTraceId(params)
  const content = params.content

  const p = getProvider()
  const processed = preprocess(content)
  logContentPreview('judge_value', content.length, processed, traceId)

  const result = await p.distill(PROMPT_B_LIGHT, processed, 'judge_value', traceId)
  console.error(
    distillLog(
      traceId,
      `judge_value: LLM 响应 ${result.content.length} 字符，tokens: in=${result.promptTokens} out=${result.completionTokens}`,
    ),
  )

  const parsed = extractJson(result.content, traceId) as {
    value: string
    type: string
    reason: string
  }
  console.error(
    distillLog(
      traceId,
      `judge_value 结果: value=${parsed.value}, type=${parsed.type}`,
    ),
  )

  return {
    ...parsed,
    prompt_tokens: result.promptTokens,
    completion_tokens: result.completionTokens,
  }
}

/**
 * distill_full handler —— 完整技术笔记提炼。
 * 输入：{ content: string }
 * 输出：{ title, type, value, summary, note, tags, tech_stack, prompt_tokens, completion_tokens }
 */
export async function handleDistillFull(
  params: Record<string, unknown>,
): Promise<DistillFullResult> {
  if (typeof params.content !== 'string' || !params.content.trim()) {
    throw new Error('参数缺失或类型错误: content 必须为非空字符串')
  }
  const traceId = resolveTraceId(params)
  const content = params.content

  const p = getProvider()
  const processed = preprocess(content)
  logContentPreview('distill_full', content.length, processed, traceId)

  const result = await p.distill(PROMPT_B_FULL, processed, 'distill_full', traceId)
  console.error(
    distillLog(
      traceId,
      `distill_full: LLM 响应 ${result.content.length} 字符，tokens: in=${result.promptTokens} out=${result.completionTokens}`,
    ),
  )

  const rawObj = extractJson(result.content, traceId) as Record<string, unknown>
  const parsed = normalizeDistillFullParsed(rawObj, traceId)
  console.error(
    distillLog(
      traceId,
      `distill_full 结果: value=${parsed.value}, type=${parsed.type}, title="${parsed.title}", tech_stack=${JSON.stringify(parsed.tech_stack)}`,
    ),
  )

  return {
    ...parsed,
    prompt_tokens: result.promptTokens,
    completion_tokens: result.completionTokens,
  }
}

/**
 * init handler —— 初始化 Provider 配置。
 * 由 Rust 侧在 sidecar 启动后调用。
 *
 * 输入：{ provider, base_url, api_key, model, timeout_secs? }
 */
export async function handleInit(
  params: Record<string, unknown>,
): Promise<{ status: string }> {
  const apiConfig = parseApiConfig(params)
  initApiProvider(apiConfig)
  return { status: 'ok' }
}

function parseApiConfig(params: Record<string, unknown>): ApiProviderConfig {
  if (typeof params.api_key !== 'string' || !params.api_key.trim()) {
    throw new Error('参数缺失或类型错误: 需要 api_key 字段')
  }
  return {
    provider: typeof params.provider === 'string' ? params.provider : 'openai-compatible',
    baseUrl: typeof params.base_url === 'string' ? params.base_url : 'https://api.openai.com/v1',
    apiKey: params.api_key,
    model: typeof params.model === 'string' ? params.model : 'gpt-4o-mini',
    timeoutMs: typeof params.timeout_secs === 'number' ? params.timeout_secs * 1000 : undefined,
  }

}

export async function handleListModels(params: Record<string, unknown>): Promise<string[]> {
  return listApiModels(parseApiConfig(params))
}

export async function handleTestProvider(params: Record<string, unknown>): Promise<string> {
  const config = parseApiConfig(params)
  try {
    await listApiModels(config)
    return '模型列表接口连接成功'
  } catch (modelsError) {
    if (!config.model.trim()) throw modelsError
    await new ApiProvider(config).testConnection()
    return '最小 Chat Completions 请求成功（供应商未提供可用的模型列表接口）'
  }
}
