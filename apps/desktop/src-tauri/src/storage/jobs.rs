use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};
use uuid::Uuid;

use super::db::{Database, DbError, DbResult};
use super::models::{Job, JobItem, NewJobItem};

fn item_from_row(row: &Row<'_>) -> rusqlite::Result<JobItem> {
    Ok(JobItem {
        id: row.get(0)?,
        job_id: row.get(1)?,
        session_id: row.get(2)?,
        source_id: row.get(3)?,
        raw_path: row.get(4)?,
        status: row.get(5)?,
        phase: row.get(6)?,
        duration_ms: row.get(7)?,
        error: row.get(8)?,
        created_at: row.get(9)?,
        started_at: row.get(10)?,
        finished_at: row.get(11)?,
    })
}

fn job_from_row(row: &Row<'_>) -> rusqlite::Result<Job> {
    Ok(Job {
        id: row.get(0)?,
        kind: row.get(1)?,
        status: row.get(2)?,
        phase: row.get(3)?,
        done: row.get(4)?,
        total: row.get(5)?,
        cancel_requested: row.get::<_, i64>(6)? != 0,
        error: row.get(7)?,
        provider_profile_id: row.get(8)?,
        provider: row.get(9)?,
        base_url: row.get(10)?,
        model: row.get(11)?,
        created_at: row.get(12)?,
        started_at: row.get(13)?,
        finished_at: row.get(14)?,
        items: Vec::new(),
    })
}

const JOB_COLUMNS: &str = "id,kind,status,phase,done,total,cancel_requested,error,provider_profile_id,provider,base_url,model,created_at,started_at,finished_at";
const ITEM_COLUMNS: &str = "id,job_id,session_id,source_id,raw_path,status,phase,duration_ms,error,created_at,started_at,finished_at";

impl Database {
    pub fn create_job(
        &self,
        kind: &str,
        phase: &str,
        profile: Option<(&str, &str, &str, &str)>,
        items: &[NewJobItem<'_>],
    ) -> DbResult<Job> {
        if items.is_empty() {
            return Err(DbError::Invalid("任务至少需要一个条目".into()));
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let (profile_id, provider, base_url, model) = profile
            .map(|value| (Some(value.0), Some(value.1), Some(value.2), Some(value.3)))
            .unwrap_or((None, None, None, None));
        tx.execute(
            "INSERT INTO jobs(id,kind,status,phase,total,provider_profile_id,provider,base_url,model,created_at)
             VALUES(?1,?2,'queued',?3,?4,?5,?6,?7,?8,?9)",
            params![id, kind, phase, items.len() as i64, profile_id, provider, base_url, model, now],
        )?;
        for item in items {
            tx.execute(
                "INSERT INTO job_items(id,job_id,session_id,source_id,raw_path,status,phase,created_at)
                 VALUES(?1,?2,?3,?4,?5,'queued','queued',?6)",
                params![Uuid::new_v4().to_string(), id, item.session_id, item.source_id, item.raw_path, now],
            )?;
        }
        tx.commit()?;
        self.get_job(&id)
    }

    pub fn get_job(&self, id: &str) -> DbResult<Job> {
        let conn = self.read_conn()?;
        let mut job = conn
            .query_row(
                &format!("SELECT {JOB_COLUMNS} FROM jobs WHERE id=?1"),
                params![id],
                job_from_row,
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("job {id}")))?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {ITEM_COLUMNS} FROM job_items WHERE job_id=?1 ORDER BY created_at,id"
        ))?;
        job.items = stmt
            .query_map(params![id], item_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(job)
    }

    pub fn list_jobs(&self, active_only: bool) -> DbResult<Vec<Job>> {
        let conn = self.read_conn()?;
        let sql = if active_only {
            format!("SELECT {JOB_COLUMNS} FROM jobs WHERE status IN ('queued','running','interrupted') ORDER BY created_at DESC")
        } else {
            format!("SELECT {JOB_COLUMNS} FROM jobs ORDER BY created_at DESC LIMIT 50")
        };
        let mut stmt = conn.prepare(&sql)?;
        let mut jobs = stmt
            .query_map([], job_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        for job in &mut jobs {
            let mut items_stmt = conn.prepare(&format!(
                "SELECT {ITEM_COLUMNS} FROM job_items WHERE job_id=?1 ORDER BY created_at,id"
            ))?;
            job.items = items_stmt
                .query_map(params![job.id], item_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
        }
        Ok(jobs)
    }

    pub fn mark_job_running(&self, id: &str, phase: &str) -> DbResult<()> {
        self.conn().execute(
            "UPDATE jobs SET status='running',phase=?1,started_at=COALESCE(started_at,?2) WHERE id=?3 AND status IN ('queued','interrupted')",
            params![phase, Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    pub fn mark_item_running(&self, id: &str, phase: &str) -> DbResult<()> {
        self.conn().execute(
            "UPDATE job_items SET status='running',phase=?1,started_at=?2 WHERE id=?3",
            params![phase, Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    pub fn update_job_item_phase(&self, job_id: &str, item_id: &str, phase: &str) -> DbResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE job_items SET phase=?1 WHERE id=?2",
            params![phase, item_id],
        )?;
        tx.execute(
            "UPDATE jobs SET phase=?1 WHERE id=?2",
            params![phase, job_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn finish_item(
        &self,
        job_id: &str,
        item_id: &str,
        status: &str,
        phase: &str,
        duration_ms: i64,
        error: Option<&str>,
    ) -> DbResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE job_items SET status=?1,phase=?2,duration_ms=?3,error=?4,finished_at=?5 WHERE id=?6",
            params![status, phase, duration_ms, error, Utc::now().to_rfc3339(), item_id],
        )?;
        tx.execute(
            "UPDATE jobs SET done=done+1,phase=?1 WHERE id=?2",
            params![phase, job_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn finish_job(&self, id: &str, status: &str, error: Option<&str>) -> DbResult<()> {
        self.conn().execute(
            "UPDATE jobs SET status=?1,phase=?1,error=?2,finished_at=?3 WHERE id=?4",
            params![status, error, Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    pub fn request_job_cancel(&self, id: &str) -> DbResult<()> {
        let changed = self.conn().execute(
            "UPDATE jobs SET cancel_requested=1 WHERE id=?1 AND status IN ('queued','running')",
            params![id],
        )?;
        if changed == 0 {
            return Err(DbError::Invalid("任务已结束，不能取消".into()));
        }
        Ok(())
    }

    pub fn job_cancel_requested(&self, id: &str) -> DbResult<bool> {
        self.read_conn()?
            .query_row(
                "SELECT cancel_requested FROM jobs WHERE id=?1",
                params![id],
                |row| row.get::<_, i64>(0).map(|v| v != 0),
            )
            .map_err(DbError::from)
    }

    pub fn cancel_queued_items(&self, job_id: &str) -> DbResult<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let count = tx.execute(
            "UPDATE job_items SET status='cancelled',phase='cancelled',finished_at=?1 WHERE job_id=?2 AND status='queued'",
            params![Utc::now().to_rfc3339(), job_id],
        )?;
        tx.execute(
            "UPDATE jobs SET done=done+?1 WHERE id=?2",
            params![count as i64, job_id],
        )?;
        tx.commit()?;
        Ok(count)
    }

    pub fn interrupt_running_jobs(&self) -> DbResult<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let now = Utc::now().to_rfc3339();
        let items = tx.execute(
            "UPDATE job_items SET status='interrupted',phase='interrupted',finished_at=?1 WHERE status='running'", params![now],
        )?;
        tx.execute(
            "UPDATE jobs SET status='interrupted',phase='interrupted',finished_at=?1 WHERE status='running'", params![now],
        )?;
        tx.execute(
            "UPDATE jobs SET done=(SELECT COUNT(*) FROM job_items WHERE job_id=jobs.id AND status IN ('succeeded','failed','cancelled','interrupted')) WHERE status='interrupted'", [],
        )?;
        tx.commit()?;
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_job_and_items_without_api_key_column() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("jobs.db")).unwrap();
        // FK 约束要求真实会话。
        let session = db
            .insert_session(
                "codex",
                "e1",
                "local",
                None,
                None,
                0,
                None,
                "/tmp/x",
                "2026-01-01T00:00:00Z",
                "2026-01-01T00:00:00Z",
                None,
            )
            .unwrap();
        let items = [NewJobItem {
            session_id: Some(&session),
            source_id: None,
            raw_path: None,
        }];
        let job = db
            .create_job(
                "analysis",
                "queued",
                Some((
                    "p1",
                    "siliconflow",
                    "https://api.siliconflow.cn/v1",
                    "model",
                )),
                &items,
            )
            .unwrap();
        assert_eq!(job.total, 1);
        let columns: Vec<String> = db
            .read_conn()
            .unwrap()
            .prepare("PRAGMA table_info(jobs)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!columns.iter().any(|column| column == "api_key"));
    }
}
