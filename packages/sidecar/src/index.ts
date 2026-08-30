/**
 * 寻迹 Sidecar 入口 —— JSON-RPC 2.0 Server。
 *
 * Rust 通过 stdin/stdout 与本进程通信，协议为 JSON-RPC 2.0。
 * 每行一个 JSON 请求，每行一个 JSON 响应。
 * 日志输出到 stderr，不会污染通信通道。
 *
 * 可用方法：
 *   ping          → { status, provider?, model? }
 *   init          → 初始化 API Provider 配置
 *   judge_value   → 轻量价值判断
 *   distill_full  → 完整技术笔记提炼
 */

import { startRpcServer, type Handler } from './rpc'
import {
  handleInit, handleJudgeValue, handleDistillFull, handleListModels, handleTestProvider,
} from './distiller'

const handlers: Record<string, Handler> = {
  ping: async () => ({ status: 'ok', version: '0.1.0' }),
  init: handleInit,
  list_models: handleListModels,
  test_provider: handleTestProvider,
  judge_value: handleJudgeValue,
  distill_full: handleDistillFull,
}

startRpcServer(handlers)
