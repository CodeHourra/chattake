use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use super::db::{Database, DbResult};

impl Database {
    pub fn create_analysis_run(
        &self,
        job_id: &str,
        session_id: &str,
        profile_id: &str,
        provider: &str,
        base_url_host: &str,
        model: &str,
        content_hash: &str,
        prompt_version: &str,
    ) -> DbResult<String> {
        let id = Uuid::new_v4().to_string();
        self.conn().execute(
            "INSERT INTO analysis_runs(id,job_id,session_id,provider_profile_id,provider,base_url_host,model,content_hash,prompt_version,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![id, job_id, session_id, profile_id, provider, base_url_host, model, content_hash, prompt_version, Utc::now().to_rfc3339()],
        )?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish_analysis_run(
        &self,
        id: &str,
        value: Option<&str>,
        reason: Option<&str>,
        judge_tokens: (i64, i64),
        extract_tokens: (i64, i64),
        duration_ms: i64,
        error_kind: Option<&str>,
        error: Option<&str>,
    ) -> DbResult<()> {
        self.conn().execute(
            "UPDATE analysis_runs SET value=?1,reason=?2,prompt_tokens_judge=?3,completion_tokens_judge=?4,
             prompt_tokens_extract=?5,completion_tokens_extract=?6,duration_ms=?7,error_kind=?8,error=?9,finished_at=?10 WHERE id=?11",
            params![value, reason, judge_tokens.0, judge_tokens.1, extract_tokens.0, extract_tokens.1,
                duration_ms, error_kind, error, Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    pub fn session_has_cards(&self, session_id: &str) -> DbResult<bool> {
        Ok(self.read_conn()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM cards WHERE session_id=?1)",
            params![session_id],
            |row| row.get::<_, i64>(0),
        )? != 0)
    }
}
