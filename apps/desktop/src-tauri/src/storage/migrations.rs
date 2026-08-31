use rusqlite::Connection;

use super::db::DbResult;

pub const SCHEMA_VERSION: u32 = 7;

pub fn run(conn: &Connection) -> DbResult<()> {
    let version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    if version > SCHEMA_VERSION {
        return Err(super::db::DbError::Invalid(format!(
            "数据库版本 v{version} 高于当前应用支持的 v{SCHEMA_VERSION}"
        )));
    }
    if version > 0 {
        log::warn!("将已备份的 v{} 数据库重建为 v{}", version, SCHEMA_VERSION);
    }
    rebuild_v7(conn)
}

fn rebuild_v7(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys=OFF;
        DROP TABLE IF EXISTS cards_fts;
        DROP TABLE IF EXISTS card_tags;
        DROP TABLE IF EXISTS tags;
        DROP TABLE IF EXISTS analysis_runs;
        DROP TABLE IF EXISTS job_items;
        DROP TABLE IF EXISTS jobs;
        DROP TABLE IF EXISTS token_usage;
        DROP TABLE IF EXISTS sync_log;
        DROP TABLE IF EXISTS categories;
        DROP TABLE IF EXISTS cards;
        DROP TABLE IF EXISTS messages;
        DROP TABLE IF EXISTS sessions;
        DROP TABLE IF EXISTS sources;

        CREATE TABLE sessions (
            id                  TEXT PRIMARY KEY,
            source_id           TEXT NOT NULL,
            external_session_id TEXT NOT NULL,
            source_host         TEXT NOT NULL DEFAULT 'local',
            project_path        TEXT,
            project_name        TEXT,
            message_count       INTEGER NOT NULL DEFAULT 0,
            content_hash        TEXT,
            raw_path            TEXT,
            raw_mtime_ms        INTEGER,
            raw_size_bytes      INTEGER,
            created_at          TEXT NOT NULL,
            updated_at          TEXT NOT NULL,
            status              TEXT NOT NULL DEFAULT 'pending',
            value               TEXT,
            has_updates         INTEGER NOT NULL DEFAULT 0,
            analyzed_at         TEXT,
            error_message       TEXT,
            analysis_title      TEXT,
            analysis_type       TEXT,
            analysis_note       TEXT,
            UNIQUE(source_id, external_session_id, source_host)
        );

        CREATE TABLE messages (
            id          TEXT PRIMARY KEY,
            session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            role        TEXT NOT NULL,
            content     TEXT NOT NULL,
            timestamp   TEXT,
            tokens_in   INTEGER NOT NULL DEFAULT 0,
            tokens_out  INTEGER NOT NULL DEFAULT 0,
            seq_order   INTEGER NOT NULL,
            UNIQUE(session_id, seq_order)
        );

        CREATE TABLE jobs (
            id                  TEXT PRIMARY KEY,
            kind                TEXT NOT NULL,
            status              TEXT NOT NULL CHECK(status IN ('queued','running','succeeded','failed','cancelled','interrupted')),
            phase               TEXT NOT NULL,
            done                INTEGER NOT NULL DEFAULT 0,
            total               INTEGER NOT NULL DEFAULT 0,
            cancel_requested    INTEGER NOT NULL DEFAULT 0,
            error               TEXT,
            provider_profile_id TEXT,
            provider            TEXT,
            base_url            TEXT,
            model               TEXT,
            created_at          TEXT NOT NULL,
            started_at          TEXT,
            finished_at         TEXT
        );

        CREATE TABLE job_items (
            id          TEXT PRIMARY KEY,
            job_id      TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
            session_id  TEXT REFERENCES sessions(id) ON DELETE SET NULL,
            source_id   TEXT,
            raw_path    TEXT,
            status      TEXT NOT NULL CHECK(status IN ('queued','running','succeeded','failed','cancelled','interrupted')),
            phase       TEXT NOT NULL,
            duration_ms INTEGER,
            error       TEXT,
            created_at  TEXT NOT NULL,
            started_at  TEXT,
            finished_at TEXT
        );

        CREATE TABLE analysis_runs (
            id                    TEXT PRIMARY KEY,
            job_id                TEXT REFERENCES jobs(id) ON DELETE SET NULL,
            session_id            TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            provider_profile_id   TEXT NOT NULL,
            provider              TEXT NOT NULL,
            base_url_host         TEXT NOT NULL,
            model                 TEXT NOT NULL,
            content_hash          TEXT NOT NULL,
            prompt_version        TEXT NOT NULL,
            value                 TEXT,
            reason                TEXT,
            prompt_tokens_judge   INTEGER NOT NULL DEFAULT 0,
            completion_tokens_judge INTEGER NOT NULL DEFAULT 0,
            prompt_tokens_extract INTEGER NOT NULL DEFAULT 0,
            completion_tokens_extract INTEGER NOT NULL DEFAULT 0,
            duration_ms           INTEGER,
            error_kind            TEXT,
            error                 TEXT,
            created_at            TEXT NOT NULL,
            finished_at           TEXT
        );

        CREATE TABLE cards (
            id                  TEXT PRIMARY KEY,
            session_id          TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            analysis_run_id     TEXT REFERENCES analysis_runs(id) ON DELETE SET NULL,
            title               TEXT NOT NULL,
            type                TEXT NOT NULL CHECK(type IN ('decision','troubleshooting','implementation','explanation','snippet')),
            value               TEXT NOT NULL CHECK(value IN ('high','medium')),
            summary             TEXT NOT NULL,
            note                TEXT NOT NULL,
            publication_status  TEXT NOT NULL CHECK(publication_status IN ('draft','published')),
            is_user_edited      INTEGER NOT NULL DEFAULT 0,
            source_name         TEXT,
            project_name        TEXT,
            prompt_tokens       INTEGER NOT NULL DEFAULT 0,
            completion_tokens   INTEGER NOT NULL DEFAULT 0,
            cost_yuan           REAL NOT NULL DEFAULT 0,
            feedback            TEXT,
            created_at          TEXT NOT NULL,
            updated_at          TEXT NOT NULL
        );

        CREATE TABLE tags (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            normalized_name TEXT NOT NULL,
            kind            TEXT NOT NULL CHECK(kind IN ('topic','technology')),
            UNIQUE(kind, normalized_name)
        );

        CREATE TABLE card_tags (
            card_id TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
            tag_id  TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY(card_id, tag_id)
        );

        CREATE VIRTUAL TABLE cards_fts USING fts5(
            title, summary, note, tags, technologies,
            tokenize='trigram'
        );

        CREATE INDEX idx_sessions_source_updated ON sessions(source_id, updated_at DESC);
        CREATE INDEX idx_sessions_project ON sessions(project_name);
        CREATE INDEX idx_sessions_status_updated ON sessions(status, updated_at DESC);
        CREATE INDEX idx_messages_session_order ON messages(session_id, seq_order);
        CREATE INDEX idx_cards_session_status ON cards(session_id, publication_status, created_at DESC);
        CREATE INDEX idx_cards_type_status ON cards(type, publication_status);
        CREATE INDEX idx_cards_value_status ON cards(value, publication_status);
        CREATE INDEX idx_card_tags_tag ON card_tags(tag_id, card_id);
        CREATE INDEX idx_jobs_status_created ON jobs(status, created_at DESC);
        CREATE INDEX idx_job_items_job_status ON job_items(job_id, status);
        CREATE INDEX idx_analysis_runs_session_created ON analysis_runs(session_id, created_at DESC);

        PRAGMA user_version=7;
        PRAGMA foreign_keys=ON;
        "#,
    )?;
    log::info!("数据库 v7 Schema 已创建");
    Ok(())
}
