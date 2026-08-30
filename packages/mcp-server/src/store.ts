import { Database } from 'bun:sqlite'
import { homedir } from 'node:os'
import { join } from 'node:path'

type Filters = { type?: string; topics?: string[]; technologies?: string[]; limit?: number }

const labelSql = (kind: 'topic' | 'technology') =>
  `(SELECT json_group_array(t.name) FROM tags t JOIN card_tags ct ON ct.tag_id=t.id WHERE ct.card_id=c.id AND t.kind='${kind}')`

function filterSql(filters: Filters, params: unknown[]) {
  const clauses = ["c.publication_status='published'"]
  if (filters.type) { clauses.push('c.type=?'); params.push(filters.type) }
  for (const [kind, names] of [['topic', filters.topics], ['technology', filters.technologies]] as const) {
    for (const name of [...new Set(names ?? [])]) {
      clauses.push('EXISTS (SELECT 1 FROM card_tags ct JOIN tags t ON t.id=ct.tag_id WHERE ct.card_id=c.id AND t.kind=? AND t.normalized_name=lower(?))')
      params.push(kind, name.trim())
    }
  }
  return clauses.join(' AND ')
}

function likePattern(value: string) {
  return `%${value.replaceAll('\\', '\\\\').replaceAll('%', '\\%').replaceAll('_', '\\_')}%`
}

function labelsAsArrays(row: Record<string, unknown> | null) {
  if (!row) return null
  for (const key of ['topics', 'technologies']) {
    row[key] = JSON.parse(String(row[key] || '[]'))
  }
  return row
}

export function defaultDatabasePath() {
  return process.env.CHATTAKE_DB || join(homedir(), '.chattake', 'db', 'chattake.db')
}

export class ChatTakeStore {
  private readonly db: Database

  constructor(path = defaultDatabasePath()) {
    this.db = new Database(path, { readonly: true, strict: true })
    this.db.exec('PRAGMA query_only=ON; PRAGMA busy_timeout=5000;')
  }

  close() { this.db.close() }

  search(query: string, filters: Filters = {}) {
    const text = query.trim()
    if (!text) return []
    const params: unknown[] = []
    const where = filterSql(filters, params)
    const topics = labelSql('topic')
    const technologies = labelSql('technology')
    const limit = Math.min(Math.max(filters.limit ?? 10, 1), 50)
    const common = `SELECT c.id,c.title,c.type,c.value,c.summary,s.source_id,s.external_session_id,s.project_name,${topics} topics,${technologies} technologies`
    const fts = [...text].length >= 3
    const sql = fts
      ? `${common},snippet(cards_fts,-1,'<mark>','</mark>','…',24) match_snippet FROM cards c JOIN sessions s ON s.id=c.session_id JOIN cards_fts ON cards_fts.rowid=c.rowid WHERE cards_fts MATCH ? AND ${where} ORDER BY bm25(cards_fts) LIMIT ?`
      : `${common},substr(c.title||char(10)||c.summary||char(10)||c.note,1,360) match_snippet FROM cards c JOIN sessions s ON s.id=c.session_id WHERE lower(c.title||char(10)||c.summary||char(10)||c.note||char(10)||${topics}||char(10)||${technologies}) LIKE lower(?) ESCAPE '\\' AND ${where} ORDER BY c.updated_at DESC LIMIT ?`
    const first = fts ? `"${text.replaceAll('"', '""')}"` : likePattern(text)
    return (this.db.query(sql).all(first, ...params, limit) as Record<string, unknown>[]).map((row) => labelsAsArrays(row))
  }

  getCard(cardId: string) {
    const topics = labelSql('topic')
    const technologies = labelSql('technology')
    return labelsAsArrays(this.db.query(`SELECT c.id,c.title,c.type,c.value,c.summary,c.note,c.source_name,c.project_name,c.created_at,c.updated_at,s.source_id,s.external_session_id,${topics} topics,${technologies} technologies FROM cards c JOIN sessions s ON s.id=c.session_id WHERE c.id=? AND c.publication_status='published'`).get(cardId) as Record<string, unknown> | null)
  }

  listFacets() {
    const countBy = (kind: string) => this.db.query("SELECT t.name,COUNT(*) count FROM tags t JOIN card_tags ct ON ct.tag_id=t.id JOIN cards c ON c.id=ct.card_id WHERE t.kind=? AND c.publication_status='published' GROUP BY t.id ORDER BY count DESC,t.name").all(kind)
    return {
      types: this.db.query("SELECT type name,COUNT(*) count FROM cards WHERE publication_status='published' GROUP BY type ORDER BY count DESC,type").all(),
      topics: countBy('topic'), technologies: countBy('technology'),
    }
  }

  getSource(cardId: string, cursor = -1, limit = 50) {
    const card = this.db.query("SELECT c.session_id,s.source_id,s.external_session_id,s.project_name FROM cards c JOIN sessions s ON s.id=c.session_id WHERE c.id=? AND c.publication_status='published'").get(cardId) as Record<string, unknown> | null
    if (!card) return null
    const size = Math.min(Math.max(limit, 1), 100)
    const messages = this.db.query('SELECT role,content,timestamp,seq_order FROM messages WHERE session_id=? AND seq_order>? ORDER BY seq_order LIMIT ?').all(card.session_id, cursor, size + 1) as Record<string, unknown>[]
    const hasMore = messages.length > size
    if (hasMore) messages.pop()
    return { source_id: card.source_id, external_session_id: card.external_session_id, project_name: card.project_name, messages, next_cursor: hasMore ? messages.at(-1)?.seq_order : null }
  }
}
