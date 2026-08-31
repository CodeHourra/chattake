use rusqlite::Row;

use super::cards::{build_card_where, card_summary_from_row, CARD_SUMMARY_COLUMNS};
use super::db::{Database, DbResult};
use super::models::*;

fn labels(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn search_row(row: &Row<'_>, query: &str, fts: bool) -> rusqlite::Result<CardSummary> {
    let mut card = card_summary_from_row(row)?;
    let note: String = row.get(11)?;
    card.tags = labels(row.get(12)?);
    card.technologies = labels(row.get(13)?);
    card.match_snippet = if fts {
        row.get(14)?
    } else {
        Some(text_snippet(
            &format!(
                "{}\n{}\n{}",
                card.title,
                card.summary.as_deref().unwrap_or_default(),
                note
            ),
            query,
        ))
    };
    Ok(card)
}

fn text_snippet(text: &str, query: &str) -> String {
    let index = text
        .find(query)
        .or_else(|| text.to_lowercase().find(&query.to_lowercase()))
        .unwrap_or(0);
    let mut start = index.saturating_sub(80);
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (index + query.len() + 120).min(text.len());
    while end < text.len() && !text.is_char_boundary(end) {
        end += 1;
    }
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        &text[start..end],
        if end < text.len() { "…" } else { "" }
    )
}

impl Database {
    pub fn search_cards(&self, query: &str, filters: &CardFilters) -> DbResult<Vec<CardSummary>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let (where_sql, filter_params) = build_card_where(filters);
        let filters_sql = where_sql.strip_prefix(" WHERE ").unwrap_or(&where_sql);
        let tag_sql = "(SELECT GROUP_CONCAT(t.name, ',') FROM tags t JOIN card_tags ct ON ct.tag_id=t.id WHERE ct.card_id=c.id AND t.kind='topic')";
        let tech_sql = "(SELECT GROUP_CONCAT(t.name, ',') FROM tags t JOIN card_tags ct ON ct.tag_id=t.id WHERE ct.card_id=c.id AND t.kind='technology')";
        let use_fts = query.chars().count() >= 3;
        let sql = if use_fts {
            format!("SELECT {CARD_SUMMARY_COLUMNS},c.note,{tag_sql},{tech_sql},snippet(cards_fts,-1,'<mark>','</mark>','…',24) FROM cards c JOIN cards_fts ON c.rowid=cards_fts.rowid WHERE cards_fts MATCH ? AND ({filters_sql}) ORDER BY bm25(cards_fts) LIMIT 50")
        } else {
            format!("SELECT {CARD_SUMMARY_COLUMNS},c.note,{tag_sql},{tech_sql},NULL FROM cards c WHERE lower(c.title || char(10) || c.summary || char(10) || c.note || char(10) || COALESCE({tag_sql},'') || char(10) || COALESCE({tech_sql},'')) LIKE lower(?) ESCAPE '\\' AND ({filters_sql}) ORDER BY c.updated_at DESC LIMIT 50")
        };
        let first = if use_fts {
            format!("\"{}\"", query.replace('"', "\"\""))
        } else {
            format!(
                "%{}%",
                query
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        };
        let mut values = vec![first];
        values.extend(filter_params);
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(&sql)?;
        let cards = stmt
            .query_map(rusqlite::params_from_iter(values.iter()), |row| {
                search_row(row, query, use_fts)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(cards)
    }
}

#[cfg(test)]
mod tests {
    use super::text_snippet;

    #[test]
    fn short_query_snippet_is_utf8_safe() {
        let snippet = text_snippet(
            &format!("{}知识提取{}", "前".repeat(100), "后".repeat(100)),
            "知识",
        );
        assert!(snippet.contains("知识提取"));
        assert!(snippet.starts_with('…'));
    }
}
