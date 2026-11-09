// db/clipboard.rs — 剪贴板历史仓储（仅文本）。
// 后台监听线程检测到剪贴板变化时 insert；前端按分类 chip 列出。
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::now_ms;
use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ClipEntry {
    pub id: i64,
    pub content: String,
    pub char_count: i64,
    pub created_at: i64,
    pub image_id: Option<String>,
    pub saved_prompt_id: Option<i64>,
}

fn latest_content(conn: &Connection) -> AppResult<Option<String>> {
    match conn.query_row(
        "SELECT content FROM clipboard_history ORDER BY id DESC LIMIT 1",
        [],
        |r| r.get::<_, String>(0),
    ) {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Db(e.to_string())),
    }
}

/// 插入一条剪贴板记录。
/// - 与「最新一条」内容相同则跳过（连续重复去重），返回 false。
/// - 插入后把表裁剪到最近 `limit` 条（limit<=0 表示不裁剪）。
pub fn insert(conn: &Connection, content: &str, limit: i64) -> AppResult<bool> {
    if let Some(latest) = latest_content(conn)? {
        if latest == content {
            return Ok(false);
        }
    }
    let chars = content.chars().count() as i64;
    conn.execute(
        "INSERT INTO clipboard_history (content, char_count, created_at) VALUES (?1, ?2, ?3)",
        params![content, chars, now_ms()],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;

    if limit > 0 {
        conn.execute(
            "DELETE FROM clipboard_history WHERE id NOT IN \
             (SELECT id FROM clipboard_history ORDER BY id DESC LIMIT ?1)",
            params![limit],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    }
    Ok(true)
}

pub fn list(conn: &Connection, limit: i64) -> AppResult<Vec<ClipEntry>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, content, char_count, created_at, image_id, saved_prompt_id \
             FROM clipboard_history ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    let rows = stmt
        .query_map(params![limit], |r| {
            Ok(ClipEntry {
                id: r.get(0)?,
                content: r.get(1)?,
                char_count: r.get(2)?,
                created_at: r.get(3)?,
                image_id: r.get(4)?,
                saved_prompt_id: r.get(5)?,
            })
        })
        .map_err(|e| AppError::Db(e.to_string()))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| AppError::Db(e.to_string()))?);
    }
    Ok(out)
}

pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM clipboard_history WHERE id = ?1", params![id])
        .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

pub fn clear(conn: &Connection) -> AppResult<()> {
    conn.execute("DELETE FROM clipboard_history", [])
        .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

pub fn insert_image(conn: &Connection, image_id: &str, limit: i64) -> AppResult<bool> {
    let latest: Option<String> = conn
        .query_row(
            "SELECT image_id FROM clipboard_history ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|e| AppError::Db(e.to_string()))?
        .flatten();
    if latest.as_deref() == Some(image_id) {
        return Ok(false);
    }
    conn.execute("INSERT INTO clipboard_history(content,char_count,created_at,image_id) VALUES('[图片]',0,?1,?2)", params![now_ms(), image_id]).map_err(|e| AppError::Db(e.to_string()))?;
    if limit > 0 {
        conn.execute("DELETE FROM clipboard_history WHERE id NOT IN (SELECT id FROM clipboard_history ORDER BY id DESC LIMIT ?1)", [limit]).map_err(|e| AppError::Db(e.to_string()))?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::memory_conn;

    #[test]
    fn insert_dedupes_consecutive_and_lists_newest_first() {
        let c = memory_conn();
        assert!(insert(&c, "a", 0).unwrap());
        assert!(
            !insert(&c, "a", 0).unwrap(),
            "consecutive duplicate should be skipped"
        );
        assert!(insert(&c, "b", 0).unwrap());
        let items = list(&c, 100).unwrap();
        let contents: Vec<&str> = items.iter().map(|e| e.content.as_str()).collect();
        assert_eq!(contents, vec!["b", "a"]);
        assert_eq!(items[0].char_count, 1);
    }

    #[test]
    fn insert_trims_to_limit() {
        let c = memory_conn();
        for i in 0..10 {
            insert(&c, &format!("item{i}"), 3).unwrap();
        }
        let items = list(&c, 100).unwrap();
        assert_eq!(items.len(), 3, "should keep only the most recent 3");
        assert_eq!(items[0].content, "item9");
    }

    #[test]
    fn delete_and_clear() {
        let c = memory_conn();
        insert(&c, "a", 0).unwrap();
        insert(&c, "b", 0).unwrap();
        let id = list(&c, 100).unwrap()[0].id;
        delete(&c, id).unwrap();
        assert_eq!(list(&c, 100).unwrap().len(), 1);
        clear(&c).unwrap();
        assert!(list(&c, 100).unwrap().is_empty());
    }
}
