// db/folders.rs
use rusqlite::{params, Connection};

use super::now_ms;
use crate::error::{AppError, AppResult};
use crate::models::folder::Folder;

pub fn list(conn: &Connection) -> AppResult<Vec<Folder>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, sort_order, created_at FROM folders \
             WHERE deleted_at IS NULL ORDER BY sort_order ASC, id ASC",
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Folder {
                id: r.get(0)?,
                name: r.get(1)?,
                sort_order: r.get(2)?,
                created_at: r.get(3)?,
            })
        })
        .map_err(|e| AppError::Db(e.to_string()))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| AppError::Db(e.to_string()))?);
    }
    Ok(out)
}

pub fn create(conn: &Connection, name: &str) -> AppResult<Folder> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidInput("folder name is empty".into()));
    }
    let max_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) FROM folders",
            [],
            |r| r.get(0),
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    let now = now_ms();
    let uuid = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO folders (uuid, name, sort_order, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![uuid, name, max_order + 1, now],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    let id = conn.last_insert_rowid();
    Ok(Folder {
        id,
        name: name.to_string(),
        sort_order: max_order + 1,
        created_at: now,
    })
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> AppResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidInput("folder name is empty".into()));
    }
    let n = conn
        .execute(
            "UPDATE folders SET name = ?1, updated_at = ?2, dirty = 1 WHERE id = ?3",
            params![name, now_ms(), id],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("folder {id}")));
    }
    Ok(())
}

/// 软删除文件夹，并复刻原 FK 的 `ON DELETE SET NULL`：把引用它的存活 prompt 的
/// folder_id 清空并标脏（标脏才能把「取消归类」同步给别的设备）。事务保证原子。
pub fn delete(conn: &mut Connection, id: i64) -> AppResult<()> {
    let now = now_ms();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let n = tx
        .execute(
            "UPDATE folders SET deleted_at = ?1, updated_at = ?1, dirty = 1 \
             WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("folder {id}")));
    }
    tx.execute(
        "UPDATE prompts SET folder_id = NULL, updated_at = ?1, dirty = 1 \
         WHERE folder_id = ?2 AND deleted_at IS NULL",
        params![now, id],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{memory_conn, prompts};
    use crate::models::prompt::PromptDraft;

    #[test]
    fn folder_delete_nulls_and_dirties_prompts() {
        let mut c = memory_conn();
        let f = create(&c, "work").unwrap().id;
        let p = prompts::create(
            &mut c,
            PromptDraft {
                rich_content: None,
                title: "p".into(),
                content: "b".into(),
                folder_id: Some(f),
                tag_ids: vec![],
            },
        )
        .unwrap();
        c.execute("UPDATE prompts SET dirty = 0", []).unwrap();
        delete(&mut c, f).unwrap();
        // folder 墓碑、从 list 隐藏。
        assert!(list(&c).unwrap().is_empty());
        // 引用它的 prompt：folder_id 清空 + 重新标脏。
        let (fid, dirty): (Option<i64>, i64) = c
            .query_row(
                "SELECT folder_id, dirty FROM prompts WHERE id = ?1",
                [p.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(fid, None);
        assert_eq!(dirty, 1, "un-foldered prompt re-dirtied");
    }
}

pub fn reorder(conn: &mut Connection, ordered_ids: &[i64]) -> AppResult<()> {
    let now = now_ms();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    for (i, id) in ordered_ids.iter().enumerate() {
        tx.execute(
            "UPDATE folders SET sort_order = ?1, updated_at = ?2, dirty = 1 WHERE id = ?3",
            params![i as i64, now, id],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}
