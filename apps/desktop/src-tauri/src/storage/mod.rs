//! 存储层 —— 封装 SQLite 数据库的所有读写操作。
//!
//! ```text
//! mod.rs          模块入口 + 公共导出
//! db.rs           Database 连接管理、错误类型
//! migrations.rs   Schema 版本迁移
//! models.rs       数据模型（与 SQL 表结构对应）
//! sessions.rs     会话 + 消息的 CRUD
//! cards.rs        知识卡片 CRUD + FTS5 索引维护
//! search.rs       FTS5 全文搜索
//! ```

mod cards;
pub mod db;
mod jobs;
mod migrations;
pub mod models;
mod search;
mod sessions;

// Database 是模块边界处唯一被外部直接引用的类型，其余类型通过 models:: 访问
pub use db::Database;
