//! 数据采集层 —— 从各 AI IDE 的本地存储中解析对话记录。
//!
//! ```text
//! mod.rs             模块入口 + 公共导出
//! normalizer.rs      统一数据格式（NormalizedSession / NormalizedMessage）
//! claude_code.rs     Claude Code JSONL 解析器
//! cursor.rs          Cursor SQLite + Lexical 解析器
//! codebuddy.rs       CodeBuddy 扩展会话（问渠路径：CodeBuddyExtension → history 下会话目录）
//! scheduler.rs       采集调度器 + 去重写入 + 同步日志
//! ```

pub mod claude_code;
pub mod codebuddy;
pub mod codex;
pub mod cursor;
pub mod normalizer;
pub mod scheduler;
