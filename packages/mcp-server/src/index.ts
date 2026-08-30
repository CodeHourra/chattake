import { McpServer } from '@modelcontextprotocol/server'
import { serveStdio } from '@modelcontextprotocol/server/stdio'
import * as z from 'zod/v4'
import { ChatTakeStore, defaultDatabasePath } from './store'

const result = (value: unknown) => ({ content: [{ type: 'text' as const, text: JSON.stringify(value, null, 2) }], structuredContent: { result: value } })
const failure = (error: unknown) => ({ isError: true, content: [{ type: 'text' as const, text: error instanceof Error ? error.message : String(error) }] })

function createServer() {
  const store = new ChatTakeStore()
  const server = new McpServer(
    { name: 'chattake', version: '0.2.0' },
    { instructions: '只读检索有得中已发布的知识。草稿、任务和供应商配置不可访问。' },
  )
  const readOnly = { readOnlyHint: true }

  server.registerTool('chattake_search', {
    description: '搜索已发布知识，支持类型、主题和技术项组合筛选。', annotations: readOnly,
    inputSchema: z.object({
      query: z.string().min(1),
      type: z.enum(['decision', 'troubleshooting', 'implementation', 'explanation', 'snippet']).optional(),
      topics: z.array(z.string()).optional(), technologies: z.array(z.string()).optional(),
      limit: z.number().int().min(1).max(50).default(10),
    }),
  }, async (input) => {
    try { return result(store.search(input.query, input)) } catch (error) { return failure(error) }
  })

  server.registerTool('chattake_get_card', {
    description: '读取一条已发布知识的完整正文。', annotations: readOnly,
    inputSchema: z.object({ card_id: z.string().min(1) }),
  }, async ({ card_id }) => {
    try { const card = store.getCard(card_id); return card ? result(card) : failure('未找到已发布知识') } catch (error) { return failure(error) }
  })

  server.registerTool('chattake_list_facets', {
    description: '列出已发布知识可用的类型、主题和技术项及数量。', annotations: readOnly,
  }, async () => {
    try { return result(store.listFacets()) } catch (error) { return failure(error) }
  })

  server.registerTool('chattake_get_source', {
    description: '按游标回溯一条已发布知识的原始可见对话，每页最多 100 条。', annotations: readOnly,
    inputSchema: z.object({ card_id: z.string().min(1), cursor: z.number().int().min(-1).optional(), limit: z.number().int().min(1).max(100).default(50) }),
  }, async ({ card_id, cursor, limit }) => {
    try { const source = store.getSource(card_id, cursor, limit); return source ? result(source) : failure('未找到已发布知识') } catch (error) { return failure(error) }
  })

  return server
}

try {
  const handle = serveStdio(createServer)
  console.error(`chattake-mcp: read-only database ${defaultDatabasePath()}`)
  process.on('SIGINT', () => void handle.close())
} catch (error) {
  console.error(`chattake-mcp: ${error instanceof Error ? error.message : error}`)
  process.exit(1)
}
