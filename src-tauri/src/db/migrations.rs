// db/migrations.rs — 基于 PRAGMA user_version 的简易版本化迁移。
use rusqlite::{params, Connection};

use super::schema;
use crate::error::{AppError, AppResult};

const CURRENT_VERSION: i64 = 5;

pub fn migrate(conn: &Connection) -> AppResult<()> {
    let v: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| AppError::Db(e.to_string()))?;

    if v > CURRENT_VERSION {
        return Err(AppError::Db(format!(
            "数据库版本 {v} 高于当前支持的 {CURRENT_VERSION}，请使用新版应用"
        )));
    }

    if v < 1 {
        conn.execute_batch(schema::V1)
            .map_err(|e| AppError::Db(format!("apply v1: {e}")))?;
    }

    if v < 2 {
        conn.execute_batch(schema::V2)
            .map_err(|e| AppError::Db(format!("apply v2: {e}")))?;
    }

    if v < 3 {
        conn.execute_batch(schema::V3)
            .map_err(|e| AppError::Db(format!("apply v3: {e}")))?;
        backfill_v3(conn)?;
        // sync_state 单例 + 本机 device_id（重入安全：OR IGNORE）。
        let device_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT OR IGNORE INTO sync_state (id, device_id) VALUES (1, ?1)",
            params![device_id],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    }

    if v < 4 {
        // 重建 folders/tags（删列级 UNIQUE）需先关外键，否则 DROP TABLE 会触发级联清空 prompts。
        // PRAGMA foreign_keys 在事务内是 no-op，必须在 V4 的 BEGIN 之前设置 → 单独 execute。
        conn.execute_batch("PRAGMA foreign_keys = OFF;")
            .map_err(|e| AppError::Db(e.to_string()))?;
        let res = conn
            .execute_batch(schema::V4)
            .map_err(|e| AppError::Db(format!("apply v4: {e}")));
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| AppError::Db(e.to_string()))?;
        res?;
    }

    if v < 5 {
        conn.execute_batch(schema::V5)
            .map_err(|e| AppError::Db(format!("apply v5: {e}")))?;
    }

    if v != CURRENT_VERSION {
        conn.execute_batch(&format!("PRAGMA user_version = {CURRENT_VERSION};"))
            .map_err(|e| AppError::Db(e.to_string()))?;
    }
    Ok(())
}

/// V3 回填：给既有行生成全局 uuid，并把缺失的 updated_at / created_at 兜底为有意义的值。
/// dirty 由列 DEFAULT 1 自动填，无需在此处理。只挑 uuid IS NULL ⇒ 可重入。
fn backfill_v3(conn: &Connection) -> AppResult<()> {
    for table in ["folders", "tags", "prompts", "sites"] {
        let ids: Vec<i64> = {
            let mut stmt = conn
                .prepare(&format!("SELECT id FROM {table} WHERE uuid IS NULL"))
                .map_err(|e| AppError::Db(e.to_string()))?;
            let rows = stmt
                .query_map([], |r| r.get::<_, i64>(0))
                .map_err(|e| AppError::Db(e.to_string()))?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| AppError::Db(e.to_string()))?);
            }
            v
        };
        for id in ids {
            let u = uuid::Uuid::new_v4().to_string();
            conn.execute(
                &format!("UPDATE {table} SET uuid = ?1 WHERE id = ?2"),
                params![u, id],
            )
            .map_err(|e| AppError::Db(e.to_string()))?;
        }
    }
    // updated_at / created_at 兜底（纯 SQL）：folders/sites 新加的 updated_at 默认 0，回退到
    // created_at；tags 历史无时间戳，用当前时间。
    let now = super::now_ms();
    conn.execute_batch(&format!(
        "UPDATE folders SET updated_at = COALESCE(NULLIF(updated_at, 0), created_at, {now});\n\
         UPDATE sites   SET updated_at = COALESCE(NULLIF(updated_at, 0), created_at, {now});\n\
         UPDATE tags    SET created_at = COALESCE(NULLIF(created_at, 0), {now}),\n\
                            updated_at = COALESCE(NULLIF(updated_at, 0), {now});"
    ))
    .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn future_schema_is_rejected_without_changing_version() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 6;").unwrap();
        assert!(migrate(&conn).is_err());
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 6);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        // 二次运行不应报错，且版本停在 CURRENT_VERSION。
        migrate(&conn).unwrap();
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, CURRENT_VERSION);
        // 关键表都已建立。
        for t in [
            "prompts",
            "folders",
            "tags",
            "prompt_tags",
            "settings",
            "sites",
            "clipboard_history",
            "sync_state",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [t],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {t} should exist");
        }
    }

    #[test]
    fn v3_adds_sync_columns_and_sync_state_singleton() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate(&conn).unwrap();
        // 四张可同步表都有 uuid / deleted_at / dirty 列。
        for t in ["folders", "tags", "prompts", "sites"] {
            for col in ["uuid", "deleted_at", "dirty", "updated_at"] {
                let n: i64 = conn
                    .query_row(
                        &format!("SELECT COUNT(*) FROM pragma_table_info('{t}') WHERE name = ?1"),
                        [col],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(n, 1, "{t}.{col} should exist");
            }
        }
        // sync_state 单例存在且有 device_id。
        let device_id: String = conn
            .query_row("SELECT device_id FROM sync_state WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(!device_id.is_empty());
    }

    #[test]
    fn v3_backfills_existing_v2_rows() {
        // 模拟真实升级路径：一个已有数据的 v2 库 → 升级到 v3，既有行须被回填。
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(schema::V1).unwrap();
        conn.execute_batch(schema::V2).unwrap();
        conn.execute_batch("PRAGMA user_version = 2;").unwrap();
        // 老 schema（无 sync 列）插一行。
        conn.execute(
            "INSERT INTO folders (name, sort_order, created_at) VALUES ('old', 0, 123)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let (uuid, updated_at, dirty): (Option<String>, i64, i64) = conn
            .query_row(
                "SELECT uuid, updated_at, dirty FROM folders WHERE name = 'old'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert!(
            uuid.is_some_and(|u| !u.is_empty()),
            "existing row backfilled uuid"
        );
        assert_eq!(updated_at, 123, "updated_at backfilled from created_at");
        assert_eq!(dirty, 1, "existing row queued for first push");
    }

    #[test]
    fn v4_name_unique_only_among_live_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate(&conn).unwrap();
        // 建 'work'，软删它。
        conn.execute(
            "INSERT INTO folders (name, sort_order, created_at, uuid, updated_at) \
             VALUES ('work', 0, 1, 'u1', 1)",
            [],
        )
        .unwrap();
        conn.execute("UPDATE folders SET deleted_at = 2 WHERE uuid = 'u1'", [])
            .unwrap();
        // 同名再建（存活）应成功 —— 不被墓碑挡住。
        conn.execute(
            "INSERT INTO folders (name, sort_order, created_at, uuid, updated_at) \
             VALUES ('work', 0, 3, 'u2', 3)",
            [],
        )
        .expect("recreate soft-deleted name allowed");
        // 但两个存活同名应被部分唯一索引拒绝。
        let dup = conn.execute(
            "INSERT INTO folders (name, sort_order, created_at, uuid, updated_at) \
             VALUES ('work', 0, 4, 'u3', 4)",
            [],
        );
        assert!(
            dup.is_err(),
            "two live rows with same name must be rejected"
        );
    }
}
