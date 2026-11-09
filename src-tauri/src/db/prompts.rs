// db/prompts.rs — Prompts 仓储。所有 SQL 在此文件，commands 只做参数转发。
use rusqlite::{params, Connection, Row};

use super::now_ms;
use crate::error::{AppError, AppResult};
use crate::models::prompt::{Prompt, PromptDraft, SortMode};

fn map_row(r: &Row<'_>) -> rusqlite::Result<Prompt> {
    Ok(Prompt {
        id: r.get(0)?,
        title: r.get(1)?,
        content: r.get(2)?,
        rich_content: r
            .get::<_, Option<String>>(10)?
            .map(|raw| {
                serde_json::from_str(&raw).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        10,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })
            })
            .transpose()?,
        folder_id: r.get(3)?,
        tag_ids: vec![], // 单独查询
        is_favorite: r.get::<_, i64>(4)? != 0,
        is_pinned: r.get::<_, i64>(5)? != 0,
        use_count: r.get(6)?,
        last_used_at: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

const SELECT_COLS: &str = "id, title, content, folder_id, is_favorite, is_pinned, \
    use_count, last_used_at, created_at, updated_at, rich_content";

fn order_clause(sort: SortMode) -> &'static str {
    match sort {
        SortMode::RecentUsed => {
            "is_pinned DESC, last_used_at IS NULL, last_used_at DESC, updated_at DESC"
        }
        SortMode::Created => "is_pinned DESC, created_at DESC",
        SortMode::Updated => "is_pinned DESC, updated_at DESC",
        SortMode::Title => "is_pinned DESC, title COLLATE NOCASE ASC",
    }
}

fn fetch_tag_ids(conn: &Connection, prompt_id: i64) -> AppResult<Vec<i64>> {
    let mut stmt = conn
        .prepare("SELECT tag_id FROM prompt_tags WHERE prompt_id = ?1 ORDER BY tag_id")
        .map_err(|e| AppError::Db(e.to_string()))?;
    let rows = stmt
        .query_map(params![prompt_id], |r| r.get::<_, i64>(0))
        .map_err(|e| AppError::Db(e.to_string()))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| AppError::Db(e.to_string()))?);
    }
    Ok(out)
}

pub fn list(conn: &Connection, sort: SortMode) -> AppResult<Vec<Prompt>> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM prompts WHERE deleted_at IS NULL ORDER BY {}",
        order_clause(sort)
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Db(e.to_string()))?;
    let mut prompts: Vec<Prompt> = stmt
        .query_map([], map_row)
        .map_err(|e| AppError::Db(e.to_string()))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| AppError::Db(e.to_string()))?;
    for p in prompts.iter_mut() {
        p.tag_ids = fetch_tag_ids(conn, p.id)?;
    }
    Ok(prompts)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Prompt> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM prompts WHERE id = ?1 AND deleted_at IS NULL"
        ))
        .map_err(|e| AppError::Db(e.to_string()))?;
    let mut p = stmt.query_row(params![id], map_row).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(format!("prompt {id}")),
        other => AppError::Db(other.to_string()),
    })?;
    p.tag_ids = fetch_tag_ids(conn, id)?;
    Ok(p)
}

fn replace_tags(conn: &Connection, prompt_id: i64, tag_ids: &[i64]) -> AppResult<()> {
    conn.execute(
        "DELETE FROM prompt_tags WHERE prompt_id = ?1",
        params![prompt_id],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    let mut stmt = conn
        .prepare("INSERT INTO prompt_tags (prompt_id, tag_id) VALUES (?1, ?2)")
        .map_err(|e| AppError::Db(e.to_string()))?;
    for tid in tag_ids {
        stmt.execute(params![prompt_id, tid])
            .map_err(|e| AppError::Db(e.to_string()))?;
    }
    Ok(())
}

pub fn create(conn: &mut Connection, draft: PromptDraft) -> AppResult<Prompt> {
    if draft.title.trim().is_empty() {
        return Err(AppError::InvalidInput("title is empty".into()));
    }
    let now = now_ms();
    let uuid = uuid::Uuid::new_v4().to_string();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    // dirty 由列 DEFAULT 1 自动置位 ⇒ 新建即排队 push。
    tx.execute(
        "INSERT INTO prompts (uuid, title, content, folder_id, is_favorite, is_pinned, \
         use_count, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 0, 0, 0, ?5, ?5)",
        params![uuid, draft.title, draft.content, draft.folder_id, now],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    let id = tx.last_insert_rowid();
    replace_tags(&tx, id, &draft.tag_ids)?;
    if let Some(doc) = &draft.rich_content {
        super::rich::set(&tx, id, doc)?;
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    get(conn, id)
}

pub fn update(conn: &mut Connection, id: i64, draft: PromptDraft) -> AppResult<Prompt> {
    let old = get(conn, id)?;
    if old.rich_content.is_none() && draft.rich_content.is_some() {
        // 文本转图文另存新 UUID，保留原同步条目，绝不静默替换云端内容。
        return create(conn, draft);
    }
    if old.rich_content.is_some() && draft.rich_content.is_none() {
        return Err(AppError::InvalidInput(
            "图文内容不能通过旧版纯文本接口覆盖".into(),
        ));
    }
    if draft.title.trim().is_empty() {
        return Err(AppError::InvalidInput("title is empty".into()));
    }
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let n = tx
        .execute(
            "UPDATE prompts SET title=?1, content=?2, folder_id=?3, updated_at=?4, dirty=1 \
             WHERE id=?5 AND deleted_at IS NULL",
            params![draft.title, draft.content, draft.folder_id, now_ms(), id],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("prompt {id}")));
    }
    replace_tags(&tx, id, &draft.tag_ids)?;
    if let Some(doc) = &draft.rich_content {
        super::rich::set(&tx, id, doc)?;
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    get(conn, id)
}

/// 软删除：打墓碑 + dirty，不物理删行（让别的设备能学到删除）。
/// junction 是纯本地派生表，直接清掉。事务保证原子。
pub fn delete(conn: &mut Connection, id: i64) -> AppResult<()> {
    let now = now_ms();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let n = tx
        .execute(
            "UPDATE prompts SET deleted_at = ?1, updated_at = ?1, dirty = 1 \
             WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("prompt {id}")));
    }
    tx.execute("DELETE FROM prompt_tags WHERE prompt_id = ?1", params![id])
        .map_err(|e| AppError::Db(e.to_string()))?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

pub fn toggle_favorite(conn: &Connection, id: i64) -> AppResult<Prompt> {
    let n = conn
        .execute(
            "UPDATE prompts SET is_favorite = 1 - is_favorite, updated_at = ?1, dirty = 1 \
             WHERE id = ?2 AND deleted_at IS NULL",
            params![now_ms(), id],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("prompt {id}")));
    }
    get(conn, id)
}

pub fn toggle_pin(conn: &Connection, id: i64) -> AppResult<Prompt> {
    let n = conn
        .execute(
            "UPDATE prompts SET is_pinned = 1 - is_pinned, updated_at = ?1, dirty = 1 \
             WHERE id = ?2 AND deleted_at IS NULL",
            params![now_ms(), id],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("prompt {id}")));
    }
    get(conn, id)
}

pub fn record_use(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE prompts SET use_count = use_count + 1, last_used_at = ?1 WHERE id = ?2",
        params![now_ms(), id],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::memory_conn;
    use crate::db::tags;

    fn draft(title: &str, tag_ids: Vec<i64>) -> PromptDraft {
        PromptDraft {
            rich_content: None,
            title: title.into(),
            content: "body".into(),
            folder_id: None,
            tag_ids,
        }
    }

    #[test]
    fn create_rejects_blank_title() {
        let mut c = memory_conn();
        assert!(matches!(
            create(&mut c, draft("   ", vec![])),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn create_get_roundtrips_tags() {
        let mut c = memory_conn();
        let t1 = tags::create(&c, "a", None).unwrap().id;
        let t2 = tags::create(&c, "b", None).unwrap().id;
        let p = create(&mut c, draft("hi", vec![t1, t2])).unwrap();
        let got = get(&c, p.id).unwrap();
        assert_eq!(got.title, "hi");
        assert_eq!(got.tag_ids, vec![t1, t2]);
    }

    #[test]
    fn update_replaces_tags() {
        let mut c = memory_conn();
        let t1 = tags::create(&c, "a", None).unwrap().id;
        let p = create(&mut c, draft("x", vec![t1])).unwrap();
        update(&mut c, p.id, draft("x", vec![])).unwrap();
        assert!(get(&c, p.id).unwrap().tag_ids.is_empty());
    }

    #[test]
    fn pinned_sorts_first_even_under_title_sort() {
        let mut c = memory_conn();
        let a = create(&mut c, draft("aaa", vec![])).unwrap();
        let b = create(&mut c, draft("zzz", vec![])).unwrap();
        toggle_pin(&c, b.id).unwrap();
        let listed = list(&c, SortMode::Title).unwrap();
        assert_eq!(
            listed[0].id, b.id,
            "pinned must lead even when title sort would put aaa first"
        );
        assert_eq!(listed[1].id, a.id);
    }

    #[test]
    fn record_use_increments() {
        let mut c = memory_conn();
        let p = create(&mut c, draft("x", vec![])).unwrap();
        record_use(&c, p.id).unwrap();
        record_use(&c, p.id).unwrap();
        let got = get(&c, p.id).unwrap();
        assert_eq!(got.use_count, 2);
        assert!(got.last_used_at.is_some());
    }

    #[test]
    fn delete_missing_is_not_found() {
        let mut c = memory_conn();
        assert!(matches!(delete(&mut c, 999), Err(AppError::NotFound(_))));
    }

    #[test]
    fn deleting_prompt_clears_junction() {
        let mut c = memory_conn();
        let t1 = tags::create(&c, "a", None).unwrap().id;
        let p = create(&mut c, draft("x", vec![t1])).unwrap();
        delete(&mut c, p.id).unwrap();
        // junction 被显式清空（纯本地派生表）。
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM prompt_tags WHERE prompt_id = ?1",
                params![p.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn delete_sets_tombstone_not_row() {
        let mut c = memory_conn();
        let p = create(&mut c, draft("x", vec![])).unwrap();
        delete(&mut c, p.id).unwrap();
        // 行还在（墓碑），deleted_at/dirty 已置位。
        let (deleted_at, dirty): (Option<i64>, i64) = c
            .query_row(
                "SELECT deleted_at, dirty FROM prompts WHERE id = ?1",
                params![p.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(deleted_at.is_some(), "tombstone set");
        assert_eq!(dirty, 1, "dirty queued for push");
    }

    #[test]
    fn list_and_get_hide_tombstoned() {
        let mut c = memory_conn();
        let p = create(&mut c, draft("x", vec![])).unwrap();
        delete(&mut c, p.id).unwrap();
        assert!(list(&c, SortMode::Created).unwrap().is_empty());
        assert!(matches!(get(&c, p.id), Err(AppError::NotFound(_))));
    }

    #[test]
    fn create_sets_uuid_and_dirty() {
        let mut c = memory_conn();
        let p = create(&mut c, draft("x", vec![])).unwrap();
        let (uuid, dirty): (Option<String>, i64) = c
            .query_row(
                "SELECT uuid, dirty FROM prompts WHERE id = ?1",
                params![p.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(uuid.is_some_and(|u| !u.is_empty()), "uuid generated");
        assert_eq!(dirty, 1);
    }
}
