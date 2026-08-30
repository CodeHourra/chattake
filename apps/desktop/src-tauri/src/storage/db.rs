use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection, OpenFlags};
use uuid::Uuid;

use super::migrations;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid data: {0}")]
    Invalid(String),
}

pub type DbResult<T> = Result<T, DbError>;

/// 应用数据库，封装 SQLite 连接。
///
/// 使用 Mutex 保证多线程安全（Tauri 的 invoke 在异步线程中调用）。
/// 默认路径: ~/.chattake/db/chattake.db
pub struct Database {
    conn: Mutex<Connection>,
    path: PathBuf,
    last_backup_path: Option<PathBuf>,
}

impl Database {
    /// 打开指定路径的数据库，自动创建目录、启用 WAL 模式并执行迁移。
    pub fn open(path: &Path) -> DbResult<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        log::info!("打开数据库: {}", path.display());
        let conn = Connection::open(path)?;
        let version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let last_backup_path = if version > 0 && version < migrations::SCHEMA_VERSION {
            Some(backup_before_rebuild(&conn, path, version)?)
        } else {
            None
        };
        // WAL 模式允许 MCP Server 只读并发访问；busy_timeout 避免写锁竞争时立即失败
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000;",
        )?;

        let db = Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
            last_backup_path,
        };
        db.migrate()?;
        log::info!("数据库就绪");
        Ok(db)
    }

    /// 使用默认路径 ~/.chattake/db/chattake.db 打开数据库
    pub fn open_default() -> DbResult<Self> {
        let base = dirs::home_dir().expect("无法获取用户主目录");
        let path = base.join(".chattake").join("db").join("chattake.db");
        Self::open(&path)
    }

    /// 获取数据库连接的 MutexGuard（阻塞获取锁）
    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("数据库 Mutex 已中毒")
    }

    /// 列表、搜索和详情使用独立短连接，避免长查询占用串行写锁。
    pub fn read_conn(&self) -> DbResult<Connection> {
        let conn = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.execute_batch("PRAGMA query_only=ON; PRAGMA busy_timeout=5000;")?;
        Ok(conn)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn last_backup_path(&self) -> Option<&Path> {
        self.last_backup_path.as_deref()
    }

    fn migrate(&self) -> DbResult<()> {
        migrations::run(&self.conn())
    }
}

fn backup_before_rebuild(conn: &Connection, db_path: &Path, version: u32) -> DbResult<PathBuf> {
    let parent = db_path
        .parent()
        .ok_or_else(|| DbError::Invalid("数据库路径缺少父目录".into()))?;
    let backup_dir = parent.join("backups");
    fs::create_dir_all(&backup_dir)?;
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let backup_path = backup_dir.join(format!(
        "chattake-v{version}-{stamp}-{}.db",
        &Uuid::new_v4().to_string()[..8]
    ));
    conn.execute(
        "VACUUM INTO ?1",
        params![backup_path.to_string_lossy().as_ref()],
    )?;
    log::warn!("旧数据库已备份，可恢复路径: {}", backup_path.display());
    Ok(backup_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backs_up_before_rebuilding_old_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chattake.db");
        let old = Connection::open(&path).unwrap();
        old.execute_batch(
            "CREATE TABLE legacy(id INTEGER); INSERT INTO legacy VALUES(1); PRAGMA user_version=6;",
        )
        .unwrap();
        drop(old);

        let db = Database::open(&path).unwrap();
        let backup = db.last_backup_path().expect("backup path");
        assert!(backup.exists());
        let backup_conn = Connection::open(backup).unwrap();
        assert_eq!(
            backup_conn
                .query_row("SELECT COUNT(*) FROM legacy", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.conn()
                .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            migrations::SCHEMA_VERSION
        );
    }
}
