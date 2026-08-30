/**
 * OpenAI-compatible API Provider —— 通过 HTTP 调用 LLM 做知识提炼。
 *
 * 兼容所有支持 OpenAI Chat Completions 接口的服务：
 * OpenAI、DeepSeek、Moonshot、Ollama、Azure OpenAI 等。
 */

import OpenAI from 'openai'
import { logOutgoingOpenAiChat } from './payload-log'
import { CONTENT_HINT } from './prompts'
import { distillLog } from './trace'

export interface ApiProviderConfig {
  /** API 提供商标识（仅用于日志） */
  provider: string
  /** API 基础 URL（如 https://api.deepseek.com/v1） */
  baseUrl: string
  /** API 密钥 */
  apiKey: string
  /** 模型名称（如 gpt-4o-mini、deepseek-chat） */
  model: string
  /** 请求超时（毫秒） */
  timeoutMs?: number
}

export interface DistillResult {
  /** LLM 返回的原始文本 */
  content: string
  /** 本次请求消耗的 prompt tokens */
  promptTokens: number
  /** 本次请求消耗的 completion tokens */
  completionTokens: number
}

export async function listApiModels(config: ApiProviderConfig): Promise<string[]> {
  const url = new URL(`${config.baseUrl.replace(/\/$/, '')}/models`)
  if (config.provider === 'siliconflow') {
    url.searchParams.set('type', 'text')
    url.searchParams.set('sub_type', 'chat')
  }
  const response = await fetch(url, {
    headers: { Authorization: `Bearer ${config.apiKey}` },
    signal: AbortSignal.timeout(config.timeoutMs ?? 30_000),
  })
  if (!response.ok) throw new Error(`HTTP ${response.status}: ${await response.text()}`)
  const body = await response.json() as { data?: Array<{ id?: unknown }> }
  return (body.data ?? [])
    .map((item) => typeof item.id === 'string' ? item.id : '')
    .filter(Boolean)
    .sort((a, b) => a.localeCompare(b))
}

export class ApiProvider {
  private client: OpenAI
  private model: string
  private provider: string

  constructor(config: ApiProviderConfig) {
    this.client = new OpenAI({
      baseURL: config.baseUrl,
      apiKey: config.apiKey,
      timeout: config.timeoutMs ?? 120_000,
    })
    this.model = config.model
    this.provider = config.provider
  }

  /**
   * 调用 LLM 执行提炼。
   * @param systemPrompt - 系统提示词（Prompt 模板）
   * @param content - 经过前处理的对话内容（即 HTTP messages[1].user）
   * @param callLabel - 调用场景，用于日志区分 judge_value / distill_full
   * @param traceId - 与 Rust 提炼流水线一致，贯穿日志
   */
  async distill(
    systemPrompt: string,
    content: string,
    callLabel: string = 'distill',
    traceId: string = 'unknown',
  ): Promise<DistillResult> {
    const systemFull = systemPrompt + CONTENT_HINT
    logOutgoingOpenAiChat({
      traceId,
      callLabel,
      provider: this.provider,
      model: this.model,
      systemFull,
      userFull: content,
    })

    const response = await this.client.chat.completions.create({
      model: this.model,
      messages: [
        { role: 'system', content: systemFull },
        { role: 'user', content },
      ],
      temperature: 0.3,
    })

    const text = response.choices[0]?.message?.content ?? ''
    const usage = response.usage

    console.error(
      distillLog(
        traceId,
        `Response: ${text.length} chars, tokens: ${usage?.prompt_tokens ?? 0}/${usage?.completion_tokens ?? 0}`,
      ),
    )

    return {
      content: text,
      promptTokens: usage?.prompt_tokens ?? 0,
      completionTokens: usage?.completion_tokens ?? 0,
    }
  }

  /** 检查 API 是否可用（发送一个简短的测试请求） */
  async isAvailable(): Promise<boolean> {
    try {
      await this.client.chat.completions.create({
        model: this.model,
        messages: [{ role: 'user', content: 'ping' }],
        max_tokens: 5,
      })
      return true
    } catch {
      return false
    }
  }

  async testConnection(): Promise<void> {
    await this.client.chat.completions.create({
      model: this.model,
      messages: [{ role: 'user', content: 'ping' }],
      max_tokens: 1,
    })
  }

  getInfo() {
    return { provider: this.provider, model: this.model }
  }
}
