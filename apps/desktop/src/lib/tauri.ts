/**
 * Tauri `invoke` 封装 —— 统一错误与类型，避免业务组件直接依赖字符串命令名。
 *
 * Rust 命令使用 `#[tauri::command(rename_all = "camelCase")]`，
 * 所以这里把 params 直接平铺传递即可，无需 `{ args: ... }` 包装。
 */
import { invoke } from '@tauri-apps/api/core'
import type {
  AppConfigDto,
  ApiProfileDto,
  Card,
  CardSummary,
  ListCardsParams,
  Message,
  Job,
  PaginatedResult,
  SearchCardsParams,
  Session,
  SessionGroupCount,
  SessionListParams,
  SessionFilterPayload,
  SessionSummary,
  TagCount,
  TypeCount,
} from '../types'

export const api = {
  startSync: (scope?: string | null) => invoke<Job>('start_sync', { scope: scope ?? null }),
  startAnalysis: (sessionIds: string[], providerProfileId?: string | null) =>
    invoke<Job>('start_analysis', { sessionIds, providerProfileId: providerProfileId ?? null }),
  listJobs: (activeOnly = false) => invoke<Job[]>('list_jobs', { activeOnly }),
  getJob: (jobId: string) => invoke<Job>('get_job', { jobId }),
  cancelJob: (jobId: string) => invoke<Job>('cancel_job', { jobId }),
  retryJobItem: (jobId: string, itemId: string, providerProfileId?: string | null) =>
    invoke<Job>('retry_job_item', { jobId, itemId, providerProfileId: providerProfileId ?? null }),

  /** 分页查询会话列表 */
  listSessions: (params: SessionListParams) =>
    invoke<PaginatedResult<SessionSummary>>('list_sessions', {
      source: params.source ?? null,
      host: params.host ?? null,
      project: params.project ?? null,
      status: params.status ?? null,
      page: params.page ?? null,
      pageSize: params.pageSize ?? null,
    }),

  /**
   * 按当前筛选批量删除会话（与 listSessions 条件一致）。
   * 仅删除应用库内记录，不删除本地源文件。
   */
  /** 多组筛选并集下的会话数量（与 deleteSessionsByFilterGroups 范围一致） */
  countSessionsByFilterGroups: (groups: SessionFilterPayload[]) =>
    invoke<number>('count_sessions_by_filter_groups', {
      groups: groups.map((g) => ({
        source: g.source ?? null,
        host: g.host ?? null,
        project: g.project ?? null,
        status: g.status ?? null,
        search: null,
      })),
    }),

  /** 按多组筛选并集批量删除会话（会话整理多选） */
  deleteSessionsByFilterGroups: (groups: SessionFilterPayload[]) =>
    invoke<number>('delete_sessions_by_filter_groups', {
      groups: groups.map((g) => ({
        source: g.source ?? null,
        host: g.host ?? null,
        project: g.project ?? null,
        status: g.status ?? null,
        search: null,
      })),
    }),

  /** 获取单个会话 */
  getSession: (id: string) => invoke<Session>('get_session', { id }),

  /** 获取会话所有消息（对话回放用） */
  getSessionMessages: (sessionId: string) =>
    invoke<Message[]>('get_session_messages', { sessionId }),

  /** FTS5 全文搜索卡片 */
  searchCards: (params: SearchCardsParams) =>
    invoke<CardSummary[]>('search_cards', {
      query: params.query,
      tags: params.tags ?? null,
      cardType: params.cardType ?? null,
      techStack: params.techStack ?? null,
    }),

  /** 分页查询知识卡片列表 */
  listCards: (params: ListCardsParams) =>
    invoke<PaginatedResult<CardSummary>>('list_cards', {
      tags: params.tags ?? null,
      cardType: params.cardType ?? null,
      value: params.value ?? null,
      techStack: params.techStack ?? null,
      page: params.page ?? null,
      pageSize: params.pageSize ?? null,
    }),

  /** 获取单张卡片（含 tags） */
  getCard: (id: string) => invoke<Card>('get_card', { id }),

  /** 获取会话分组统计（侧栏目录树） */
  getSessionGroups: () => invoke<SessionGroupCount[]>('get_session_groups'),

  /** 获取所有标签及数量（知识库侧栏） */
  listTags: () => invoke<TagCount[]>('list_tags'),

  /** 获取技术栈聚合及数量（来自 cards.tech_stack 列） */
  listTechStackCounts: () => invoke<TagCount[]>('list_tech_stack_counts'),

  /** 获取知识类型统计（知识库侧栏） */
  listCardTypes: () => invoke<TypeCount[]>('list_card_types'),

  /** 读取当前应用配置 */
  getConfig: () => invoke<AppConfigDto>('get_config'),

  /** 本次启动若重建过旧库，返回可恢复备份路径。 */
  getDatabaseBackupPath: () => invoke<string | null>('get_database_backup_path'),

  /** 保存应用配置（写磁盘 + 热更新内存） */
  saveConfig: (config: AppConfigDto) => invoke<void>('save_config', { config }),

  /** 临时加载供应商模型列表；结果不持久化。 */
  listProviderModels: (profile: ApiProfileDto) =>
    invoke<string[]>('list_provider_models', { profile }),

  /** 优先测试模型列表；不支持时发送最小 Chat Completions 请求。 */
  testProvider: (profile: ApiProfileDto) =>
    invoke<string>('test_provider', { profile }),

  /** 单条笔记导出为 Markdown（路径由系统「另存为」决定） */
  exportCardMarkdown: (cardId: string, filePath: string) =>
    invoke<void>('export_card_markdown', { cardId, filePath }),

  /** 按 id 列表导出多文件到目录 */
  exportCardsMarkdownDir: (cardIds: string[], dirPath: string) =>
    invoke<number>('export_cards_markdown_dir', { cardIds, dirPath }),

  /** 库内全部卡片导出到目录（无视筛选） */
  exportAllCardsMarkdownDir: (dirPath: string) =>
    invoke<number>('export_all_cards_markdown_dir', { dirPath }),

  /** 库内卡片总数 */
  countAllCards: () => invoke<number>('count_all_cards'),
}
