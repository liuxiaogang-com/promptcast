// db — 数据访问层（SQLite）。所有 SQL 在此目录内，commands/ 只做参数转发。
//
// 连接策略：单一 Connection + parking_lot::Mutex，按 Tauri State 注入。
// MVP 数据规模 <100，单连接足够；后续若需要并发可换 r2d2 池。
pub mod clipboard;
pub mod folders;
pub mod images;
pub mod migrations;
pub mod prompts;
pub mod rich;
pub mod schema;
pub mod settings;
pub mod sites;
pub mod sync_repo;
pub mod sync_state;
pub mod tags;

use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::Path;

use crate::error::{AppError, AppResult};

pub struct DbState(pub Mutex<Connection>);

impl DbState {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)
            .map_err(|e| AppError::Db(format!("open {}: {e}", path.display())))?;
        // Pragma：开启外键、WAL 模式（更稳健）。
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            "#,
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
        migrations::migrate(&conn)?;
        Ok(Self(Mutex::new(conn)))
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 测试专用：内存库 + 完整 schema（与生产同样开外键 + 迁移）。
#[cfg(test)]
pub fn memory_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("open :memory:");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrations::migrate(&conn).expect("migrate");
    conn
}
