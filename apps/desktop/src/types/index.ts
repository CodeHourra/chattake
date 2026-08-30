/**
 * 与 Rust Tauri 返回值对应的 TypeScript 类型（字段名使用 camelCase，与 serde 配置一致）
 */

export type JobStatus = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled' | 'interrupted'

export interface JobItem {
  id: string
  jobId: string
  sessionId: string | null
  sourceId: string | null
  rawPath: string | null
  status: JobStatus
  phase: string
  durationMs: number | null
  error: string | null
  createdAt: string
  startedAt: string | null
  finishedAt: string | null
}

export interface Job {
  id: string
  kind: 'sync' | 'analysis'
  status: JobStatus
  phase: string
  done: number
  total: number
  cancelRequested: boolean
  error: string | null
  providerProfileId: string | null
  provider: string | null
  baseUrl: string | null
  model: string | null
  createdAt: string
  startedAt: string | null
  finishedAt: string | null
  items: JobItem[]
}

export interface PaginatedResult<T> {
  items: T[]
  total: number
  page: number
  pageSize: number
}

export interface CursorPage<T> {
  items: T[]
  nextCursor: number | null
}

/** 列表行：会话摘要 */
export interface SessionSummary {
  id: string
  sourceId: string
  sessionId: string
  sourceHost: string
  projectPath: string | null
  projectName: string | null
  messageCount: number
  status: string
  value?: string | null
  updatedAt: string
  hasUpdates: boolean
  createdAt: string
  cardId?: string | null
  /** 所有消息内容字节总量（SUM(LENGTH(content))），用于展示 xx KB */
  rawSizeBytes: number
  /** 最新知识卡片标题（已分析后展示） */
  cardTitle?: string | null
  /** 最新知识卡片一句话摘要 */
  cardSummary?: string | null
  /** 最新知识卡片类型（英文枚举，展示用 getCardTypeLabel） */
  cardType?: string | null
  /** 最新知识卡片标签（逗号分隔，如 "Rust,SQLite"） */
  cardTags?: string | null
  /** 原始会话文件路径（如 JSONL） */
  rawPath?: string | null
  /** 分析失败时的可读原因 */
  errorMessage?: string | null
  /** 首条 user 消息预览（后端 SUBSTR），用于列表标题与 tooltip */
  firstUserPreview?: string | null
}

export interface Session {
  id: string
  sourceId: string
  sessionId: string
  sourceHost: string
  projectPath: string | null
  projectName: string | null
  /** 采集侧可读标题（如 CodeBuddy 工作区 index 中的会话 name），未提炼笔记前可用于详情/列表 */
  analysisTitle?: string | null
  messageCount: number
  contentHash: string | null
  rawPath: string | null
  createdAt: string
  updatedAt: string
  status: string
  value: string | null
  hasUpdates: boolean
  analyzedAt: string | null
  errorMessage: string | null
}

export interface Message {
  id: string
  sessionId: string
  role: string
  content: string
  timestamp: string | null
  tokensIn: number
  tokensOut: number
  seqOrder: number
}

export interface Card {
  id: string
  sessionId: string
  analysisRunId: string | null
  title: string
  type: string | null
  value: string | null
  summary: string | null
  note: string
  publicationStatus: 'draft' | 'published'
  isUserEdited: boolean
  sourceName: string | null
  projectName: string | null
  promptTokens: number
  completionTokens: number
  costYuan: number
  feedback: string | null
  createdAt: string
  updatedAt: string
  tags: string[]
  /** LLM 提炼时识别到的技术栈（如 Rust、SQLite、Tauri 等） */
  techStack: string[]
  /** 数据源侧会话标识（sessions.session_id），非会话 DB 主键 */
  sourceSessionExternalId?: string | null
  /** 来源会话路径（优先 raw_path，否则 project_path） */
  sourceSessionPath?: string | null
}

export interface CardSummary {
  id: string
  sessionId: string
  title: string
  type: string | null
  value: string | null
  summary: string | null
  publicationStatus: 'draft' | 'published'
  sourceName: string | null
  projectName: string | null
  createdAt: string
  updatedAt: string
  matchSnippet: string | null
  tags: string[]
  technologies: string[]
}

export interface SessionListParams {
  source?: string | null
  host?: string | null
  project?: string | null
  status?: string | null
  page?: number
  pageSize?: number
}

/** 与后端 `SessionFilters` 对齐，用于多组筛选删除/计数（无分页字段） */
export type SessionFilterPayload = Pick<
  SessionListParams,
  'source' | 'host' | 'project' | 'status'
>

/** 会话按 source → host → project 分组统计 */
export interface SessionGroupCount {
  sourceId: string
  sourceHost: string
  projectName: string | null
  count: number
}

/** 标签及关联卡片数量 */
export interface TagCount {
  name: string
  count: number
}

/** 知识类型及卡片数量 */
export interface TypeCount {
  name: string
  count: number
}

export interface SearchCardsParams {
  query: string
  tags?: string[] | null
  cardType?: string | null
  /** 技术栈名称列表，AND 语义，与 list_cards 一致 */
  techStack?: string[] | null
}

export interface ListCardsParams {
  tags?: string[] | null
  cardType?: string | null
  value?: string | null
  /** 技术栈名称列表，AND 语义（卡片 tech_stack 字段需同时包含所选项） */
  techStack?: string[] | null
  publicationStatus?: 'draft' | 'published'
  page?: number
  pageSize?: number
}

export interface TagRecord {
  id: string
  name: string
  normalizedName: string
  kind: 'topic' | 'technology'
}

export interface CardUpdatePayload {
  title: string
  cardType: string
  summary: string
  note: string
  tags: string[]
  technologies: string[]
}

// ─── 配置相关类型（与 Rust AppConfigDto 对应） ───────────────────────────────

export interface AppConfigDto {
  distiller: DistillerConfigDto
  collector: CollectorConfigDto
  sync: SyncConfigDto
}

export interface DistillerConfigDto {
  activeProfileId: string
  profiles: ApiProfileDto[]
}

export interface ApiProfileDto {
  id: string
  name: string
  provider: string
  baseUrl: string
  apiKey: string
  model: string
  timeoutSecs: number
}

export interface CollectorConfigDto {
  sources: SourceConfigDto[]
}

export interface SourceConfigDto {
  id: string
  name: string
  enabled: boolean
  scanDirs: string[]
}

export interface SyncConfigDto {
  scanOnStartup: boolean
}

export interface McpInfo {
  available: boolean
  binaryPath: string | null
  databasePath: string
  configSnippet: string | null
}
