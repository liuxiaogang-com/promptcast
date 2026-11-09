// db/tags.rs
use rusqlite::{params, Connection};

use super::now_ms;
use crate::error::{AppError, AppResult};
use crate::models::tag::Tag;

pub fn list(conn: &Connection) -> AppResult<Vec<Tag>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, color FROM tags WHERE deleted_at IS NULL \
             ORDER BY name COLLATE NOCASE ASC",
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Tag {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
            })
        })
        .map_err(|e| AppError::Db(e.to_string()))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| AppError::Db(e.to_string()))?);
    }
    Ok(out)
}

pub fn create(conn: &Connection, name: &str, color: Option<&str>) -> AppResult<Tag> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidInput("tag name is empty".into()));
    }
    let now = now_ms();
    let uuid = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO tags (uuid, name, color, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
        params![uuid, name, color, now],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Tag {
        id: conn.last_insert_rowid(),
        name: name.to_string(),
        color: color.map(String::from),
    })
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> AppResult<()> {
    let n = conn
        .execute(
            "UPDATE tags SET name = ?1, updated_at = ?2, dirty = 1 WHERE id = ?3",
            params![name.trim(), now_ms(), id],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("tag {id}")));
    }
    Ok(())
}

/// 软删除标签，并复刻 junction 的 `ON DELETE CASCADE`：先把仍引用它的存活 prompt 标脏
/// （它们的 tag 列表变了，需重新 push），再删 junction 行。事务保证原子。
pub fn delete(conn: &mut Connection, id: i64) -> AppResult<()> {
    let now = now_ms();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let n = tx
        .execute(
            "UPDATE tags SET deleted_at = ?1, updated_at = ?1, dirty = 1 \
             WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("tag {id}")));
    }
    tx.execute(
        "UPDATE prompts SET updated_at = ?1, dirty = 1 \
         WHERE deleted_at IS NULL AND id IN (SELECT prompt_id FROM prompt_tags WHERE tag_id = ?2)",
        params![now, id],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    tx.execute("DELETE FROM prompt_tags WHERE tag_id = ?1", params![id])
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
    fn tag_delete_removes_junction_and_dirties_prompts() {
        let mut c = memory_conn();
        let t = create(&c, "x", None).unwrap().id;
        let p = prompts::create(
            &mut c,
            PromptDraft {
                rich_content: None,
                title: "p".into(),
                content: "b".into(),
                folder_id: None,
                tag_ids: vec![t],
            },
        )
        .unwrap();
        // 清掉 create 留下的 dirty，确保下面的脏是删除标签造成的。
        c.execute("UPDATE prompts SET dirty = 0", []).unwrap();
        delete(&mut c, t).unwrap();
        // junction 清空。
        let j: i64 = c
            .query_row("SELECT COUNT(*) FROM prompt_tags", [], |r| r.get(0))
            .unwrap();
        assert_eq!(j, 0);
        // 标签是墓碑、prompt 被标脏。
        let tag_deleted: Option<i64> = c
            .query_row("SELECT deleted_at FROM tags WHERE id = ?1", [t], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(tag_deleted.is_some());
        let pd: i64 = c
            .query_row("SELECT dirty FROM prompts WHERE id = ?1", [p.id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(pd, 1, "affected prompt re-dirtied");
        // 标签从 list 隐藏。
        assert!(list(&c).unwrap().is_empty());
    }
}
