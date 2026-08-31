import { Database } from 'bun:sqlite'
import { afterAll, expect, test } from 'bun:test'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { ChatTakeStore } from './store'

const dir = mkdtempSync(join(tmpdir(), 'chattake-mcp-'))
const path = join(dir, 'test.db')
const db = new Database(path)
db.exec(`
  CREATE TABLE sessions(id TEXT PRIMARY KEY,source_id TEXT,external_session_id TEXT,project_name TEXT);
  CREATE TABLE messages(session_id TEXT,role TEXT,content TEXT,timestamp TEXT,seq_order INTEGER);
  CREATE TABLE cards(id TEXT PRIMARY KEY,session_id TEXT,title TEXT,type TEXT,value TEXT,summary TEXT,note TEXT,publication_status TEXT,source_name TEXT,project_name TEXT,created_at TEXT,updated_at TEXT);
  CREATE TABLE tags(id TEXT PRIMARY KEY,name TEXT,normalized_name TEXT,kind TEXT);
  CREATE TABLE card_tags(card_id TEXT,tag_id TEXT);
  CREATE VIRTUAL TABLE cards_fts USING fts5(title,summary,note,tags,technologies,tokenize='trigram');
  INSERT INTO sessions VALUES('s1','codex','external-1','chattake');
  INSERT INTO messages VALUES('s1','user','如何修复超时？',NULL,0),('s1','assistant','增加可取消超时。',NULL,1);
  INSERT INTO cards VALUES('published','s1','修复超时','troubleshooting','high','避免无效超时','正文','published','Codex','chattake','now','now');
  INSERT INTO cards VALUES('draft','s1','隐藏草稿','decision','medium','不应可见','草稿正文','draft','Codex','chattake','now','now');
  INSERT INTO tags VALUES('t1','Rust','rust','technology'),('t2','性能','性能','topic');
  INSERT INTO card_tags VALUES('published','t1'),('published','t2'),('draft','t2');
  INSERT INTO cards_fts(rowid,title,summary,note,tags,technologies) SELECT rowid,title,summary,note,'性能','Rust' FROM cards;
`)
db.close()
const store = new ChatTakeStore(path)

afterAll(() => { store.close(); rmSync(dir, { recursive: true, force: true }) })

test('四类只读查询仅暴露已发布知识', () => {
  expect(store.search('超时', { technologies: ['rust'] })).toMatchObject([{ topics: ['性能'], technologies: ['Rust'] }])
  expect(store.search('草稿')).toHaveLength(0)
  expect(store.getCard('draft')).toBeNull()
  expect(store.listFacets()).toEqual({ types: [{ name: 'troubleshooting', count: 1 }], topics: [{ name: '性能', count: 1 }], technologies: [{ name: 'Rust', count: 1 }] })
  const source = store.getSource('published', -1, 1)
  expect(source?.messages).toHaveLength(1)
  expect(source?.next_cursor).toBe(0)
})
