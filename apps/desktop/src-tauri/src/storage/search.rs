use super::cards::{build_card_where, card_summary_from_row, CARD_SUMMARY_COLUMNS};
use super::db::{Database, DbResult};
use super::models::*;

impl Database {
    /// FTS5 全文搜索知识卡片，支持叠加类型/价值/标签筛选。
    ///
    /// 使用 BM25 算法排序，最多返回 50 条结果。
    /// 空查询直接返回空列表。
    pub fn search_cards(&self, query: &str, filters: &CardFilters) -> DbResult<Vec<CardSummary>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }

        // 将 FTS MATCH 条件与 card_filter 的常规条件合并。
        // 注意：FTS5 要求 MATCH 左侧必须是虚拟表名（如 cards_fts），不能用 JOIN 别名 `fts`，
        // 否则任意关键词都会触发 “no such column: fts”（解析器把别名误当成列）。
        let (where_sql, filter_params) = build_card_where(filters);
        let where_clause = if where_sql.is_empty() {
            " WHERE cards_fts MATCH ?".to_string()
        } else {
            let rest = where_sql.strip_prefix(" WHERE ").unwrap_or(&where_sql);
            format!(" WHERE cards_fts MATCH ? AND ({})", rest)
        };

        let sql = format!(
            "SELECT {cols} FROM cards c \
             JOIN cards_fts fts ON c.rowid = fts.rowid \
             {where_clause} \
             ORDER BY bm25(cards_fts) LIMIT 50",
            cols = CARD_SUMMARY_COLUMNS,
        );

        // FTS MATCH 参数在最前面，后面跟 filter 参数
        let mut params_vec: Vec<String> = Vec::with_capacity(1 + filter_params.len());
        params_vec.push(q.to_string());
        params_vec.extend(filter_params);

        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(&sql)?;
        let results: Vec<CardSummary> = stmt
            .query_map(rusqlite::params_from_iter(params_vec.iter()), |r| {
                card_summary_from_row(r)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        log::debug!("全文搜索: query={:?}, 结果数={}", q, results.len());
        Ok(results)
    }
}
