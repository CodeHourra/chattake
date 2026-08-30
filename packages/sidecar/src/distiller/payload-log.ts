/**
 * 记录「即将发往 OpenAI-compatible API」的 messages 摘要，便于对照 UI 与会话原文。
 *
 * 环境变量 CHATTAKE_LOG_DISTILL_PAYLOAD=1：向 stderr 打印完整 system / user 正文（可能很长，仅排查时开启）。
 * 桌面应用开发构建（tauri dev）会在未设置时默认置为 1；设为 0 可关闭。
 */

import { Buffer } from 'node:buffer'
import { createHash } from 'node:crypto'

import { distillLog } from './trace'

/** 与 Rust 端 `log_rpc_distill_payload` 使用同一环境变量，便于同时开全文 */
export const ENV_DISTILL_PAYLOAD_FULL = 'CHATTAKE_LOG_DISTILL_PAYLOAD'

const PREVIEW_CHARS = 2500

function md5Prefix16(s: string): string {
  return createHash('md5').update(s, 'utf8').digest('hex').slice(0, 16)
}

/** 与 Rust `str::len()`（UTF-8 字节数）对齐，便于与 RPC 日志对照 */
function utf8ByteLength(s: string): number {
  return Buffer.byteLength(s, 'utf8')
}

function previewHeadTail(s: string, n: number): { head: string; tail: string; omitted: boolean } {
  if (s.length <= n * 2) {
    return { head: s, tail: '', omitted: false }
  }
  return {
    head: s.slice(0, n),
    tail: s.slice(-n),
    omitted: true,
  }
}

/**
 * API 模式：与 `chat.completions.create` 的 messages 完全一致（system 含 CONTENT_HINT_API）。
 */
export function logOutgoingOpenAiChat(args: {
  /** 与 Rust distill trace_id 一致 */
  traceId: string
  /** 调用方标识，如 judge_value / distill_full */
  callLabel: string
  provider: string
  model: string
  /** 已拼接 CONTENT_HINT_API 后的最终 system 正文 */
  systemFull: string
  /** preprocess 后的 user 正文（与 messages[1].content 一致） */
  userFull: string
}): void {
  const full = process.env[ENV_DISTILL_PAYLOAD_FULL] === '1'
  const { traceId, callLabel, provider, model, systemFull, userFull } = args
  const uMd5 = md5Prefix16(userFull)
  const sysUtf8 = utf8ByteLength(systemFull)
  const userUtf8 = utf8ByteLength(userFull)

  console.error(
    distillLog(
      traceId,
      `API 即将发送 ${callLabel}: ${provider}/${model} | system UTF-8=${sysUtf8}字节 user UTF-8=${userUtf8}字节 | user.md5[0:16]=${uMd5}（此处 user 已 preprocess；若与 Rust RPC 的 md5 不一致，说明 clean/truncate 改动了正文）`,
    ),
  )
  console.error(
    distillLog(
      traceId,
      `与 HTTP 一致: messages[0].role=system messages[1].role=user（JS 字符数 system=${systemFull.length} user=${userFull.length}）`,
    ),
  )

  if (full) {
    console.error(distillLog(traceId, `====== BEGIN system (${callLabel}) ======`))
    console.error(systemFull)
    console.error(distillLog(traceId, `====== END system ======`))
    console.error(distillLog(traceId, `====== BEGIN user (${callLabel}) ======`))
    console.error(userFull)
    console.error(distillLog(traceId, `====== END user ======`))
    return
  }

  const ps = previewHeadTail(systemFull, PREVIEW_CHARS)
  const pu = previewHeadTail(userFull, PREVIEW_CHARS)
  console.error(distillLog(traceId, `system 预览 前${PREVIEW_CHARS} 字符:\n${ps.head}`))
  if (ps.omitted && ps.tail) {
    console.error(distillLog(traceId, `system 预览 后${PREVIEW_CHARS} 字符:\n${ps.tail}`))
  }
  console.error(distillLog(traceId, `user 预览 前${PREVIEW_CHARS} 字符:\n${pu.head}`))
  if (pu.omitted && pu.tail) {
    console.error(distillLog(traceId, `user 预览 后${PREVIEW_CHARS} 字符:\n${pu.tail}`))
  }
  console.error(
    distillLog(
      traceId,
      `完整 system+user 请设置 ${ENV_DISTILL_PAYLOAD_FULL}=1 并重启 sidecar（全文可能含隐私，勿长期开启）`,
    ),
  )
}
