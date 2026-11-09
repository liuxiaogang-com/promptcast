// db/sync_repo.rs — 同步引擎的 DB 层：收集待推送(dirty)行、清 dirty、apply 服务端来的变更。
// 引擎(sync/)只管 HTTP + 顺序 + 游标；所有 SQL 在这里。
//
// 关键点：
//  - 跨设备身份用 uuid；apply 时把 folder_uuid/tag_uuids 解析成本地 int id。
//  - LWW：服务端来的变更仅当「不比本地旧、且不是本地未推送的更新」才覆盖（见 decide_write）。
//  - folders/tags 的 name 仍 UNIQUE：跨设备同名时按名收编（target by name），把本地同名行
//    收编为 incoming 的 uuid，避免 UNIQUE 冲突。极端情形（两设备各建同名）会以 LWW 收敛到
//    较新者的 uuid —— MVP 可接受。
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use super::now_ms;
use crate::error::{AppError, AppResult};
use crate::models::sync::{
    Change, FolderData, PromptData, SiteData, TagData, ENTITY_FOLDER, ENTITY_PROMPT, ENTITY_SITE,
    ENTITY_TAG,
};

fn dberr(e: rusqlite::Error) -> AppError {
    AppError::Db(e.to_string())
}

fn table_for(entity: &str) -> AppResult<&'static str> {
    Ok(match entity {
        ENTITY_FOLDER => "folders",
        ENTITY_TAG => "tags",
        ENTITY_PROMPT => "prompts",
        ENTITY_SITE => "sites",
        other => return Err(AppError::InvalidInput(format!("unknown entity {other}"))),
    })
}

/// 推送成功后清 dirty —— 仅当行仍是被推送的那个版本（updated_at 未变）。
/// 防丢更新：push 是「快照 dirty → 放锁 → HTTP」，若期间用户又改了这行（updated_at 变、
/// 重新置 dirty），无条件清 dirty 会吞掉这次新编辑；带 updated_at 守卫则保留、下拍重推。
pub fn clear_dirty(
    conn: &Connection,
    entity: &str,
    uuid: &str,
    pushed_updated_at: i64,
) -> AppResult<()> {
    let t = table_for(entity)?;
    conn.execute(
        &format!("UPDATE {t} SET dirty = 0 WHERE uuid = ?1 AND updated_at = ?2 AND dirty = 1"),
        params![uuid, pushed_updated_at],
    )
    .map_err(dberr)?;
    Ok(())
}

/// 当前 dirty 行数（用于 UI「待推送」角标）。
pub fn dirty_count(conn: &Connection) -> AppResult<i64> {
    let mut total = 0i64;
    for t in ["folders", "tags", "prompts", "sites"] {
        let n: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM {t} WHERE dirty = 1{}",
                    if t == "prompts" {
                        " AND rich_content IS NULL"
                    } else {
                        ""
                    }
                ),
                [],
                |r| r.get(0),
            )
            .map_err(dberr)?;
        total += n;
    }
    Ok(total)
}

/// 收集所有待推送(dirty)行（含墓碑）为同步信封。push 顺序无所谓（服务端逐条独立 upsert）。
pub fn collect_dirty(conn: &Connection) -> AppResult<Vec<Change>> {
    let mut out = Vec::new();

    // folders
    {
        let mut stmt = conn
            .prepare(
                "SELECT uuid, name, sort_order, created_at, updated_at, deleted_at \
                 FROM folders WHERE dirty = 1",
            )
            .map_err(dberr)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                ))
            })
            .map_err(dberr)?;
        for row in rows {
            let (uuid, name, sort_order, created_at, updated_at, deleted_at) =
                row.map_err(dberr)?;
            let Some(uuid) = uuid else { continue };
            out.push(Change {
                entity: ENTITY_FOLDER.into(),
                uuid,
                updated_at,
                deleted_at,
                data: json!({ "name": name, "sort_order": sort_order, "created_at": created_at }),
                seq: 0,
            });
        }
    }

    // tags
    {
        let mut stmt = conn
            .prepare(
                "SELECT uuid, name, color, created_at, updated_at, deleted_at \
                 FROM tags WHERE dirty = 1",
            )
            .map_err(dberr)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                ))
            })
            .map_err(dberr)?;
        for row in rows {
            let (uuid, name, color, created_at, updated_at, deleted_at) = row.map_err(dberr)?;
            let Some(uuid) = uuid else { continue };
            out.push(Change {
                entity: ENTITY_TAG.into(),
                uuid,
                updated_at,
                deleted_at,
                data: json!({ "name": name, "color": color, "created_at": created_at }),
                seq: 0,
            });
        }
    }

    // sites
    {
        let mut stmt = conn
            .prepare(
                "SELECT uuid, name, url, sort_order, created_at, updated_at, deleted_at \
                 FROM sites WHERE dirty = 1",
            )
            .map_err(dberr)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                ))
            })
            .map_err(dberr)?;
        for row in rows {
            let (uuid, name, url, sort_order, created_at, updated_at, deleted_at) =
                row.map_err(dberr)?;
            let Some(uuid) = uuid else { continue };
            out.push(Change {
                entity: ENTITY_SITE.into(),
                uuid,
                updated_at,
                deleted_at,
                data: json!({ "name": name, "url": url, "sort_order": sort_order, "created_at": created_at }),
                seq: 0,
            });
        }
    }

    // prompts —— 先取基础行，再解析 folder_uuid / tag_uuids。
    #[allow(clippy::type_complexity)]
    let prompt_rows: Vec<(
        i64,
        Option<String>,
        String,
        String,
        bool,
        bool,
        i64,
        i64,
        Option<i64>,
        Option<i64>,
    )> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, uuid, title, content, is_favorite, is_pinned, created_at, \
                 updated_at, deleted_at, folder_id FROM prompts WHERE dirty = 1 AND rich_content IS NULL",
            )
            .map_err(dberr)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)? != 0,
                    r.get::<_, i64>(5)? != 0,
                    r.get::<_, i64>(6)?,
                    r.get::<_, i64>(7)?,
                    r.get::<_, Option<i64>>(8)?,
                    r.get::<_, Option<i64>>(9)?,
                ))
            })
            .map_err(dberr)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row.map_err(dberr)?);
        }
        v
    };
    for (
        id,
        uuid,
        title,
        content,
        is_favorite,
        is_pinned,
        created_at,
        updated_at,
        deleted_at,
        folder_id,
    ) in prompt_rows
    {
        let Some(uuid) = uuid else { continue };
        let folder_uuid: Option<String> = match folder_id {
            Some(fid) => conn
                .query_row(
                    "SELECT uuid FROM folders WHERE id = ?1",
                    params![fid],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(dberr)?
                .flatten(),
            None => None,
        };
        let tag_uuids: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT t.uuid FROM prompt_tags pt JOIN tags t ON t.id = pt.tag_id \
                     WHERE pt.prompt_id = ?1 AND t.uuid IS NOT NULL ORDER BY t.uuid",
                )
                .map_err(dberr)?;
            let rows = stmt
                .query_map(params![id], |r| r.get::<_, String>(0))
                .map_err(dberr)?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(dberr)?);
            }
            v
        };
        out.push(Change {
            entity: ENTITY_PROMPT.into(),
            uuid,
            updated_at,
            deleted_at,
            data: json!({
                "title": title,
                "content": content,
                "is_favorite": is_favorite,
                "is_pinned": is_pinned,
                "created_at": created_at,
                "folder_uuid": folder_uuid,
                "tag_uuids": tag_uuids,
            }),
            seq: 0,
        });
    }

    Ok(out)
}

/// 找出本变更对应的本地行（优先 uuid，name-unique 实体退而按 name 收编）。
/// 返回 (id, updated_at, dirty, 该行现有 uuid)。
type Target = (i64, i64, i64, Option<String>);
fn find_target(
    conn: &Connection,
    table: &str,
    uuid: &str,
    name_for_collision: Option<&str>,
) -> AppResult<Option<Target>> {
    if let Some(row) = conn
        .query_row(
            &format!("SELECT id, updated_at, dirty, uuid FROM {table} WHERE uuid = ?1"),
            params![uuid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(dberr)?
    {
        return Ok(Some(row));
    }
    if let Some(name) = name_for_collision {
        return conn
            .query_row(
                &format!("SELECT id, updated_at, dirty, uuid FROM {table} WHERE name = ?1"),
                params![name],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(dberr);
    }
    Ok(None)
}

/// 收编发生（incoming uuid != 本地行旧 uuid）后，给空出来的旧 uuid 补一条墓碑，
/// 下拍 push 让服务端回收这条孤儿活记录（否则它永不更新、永久留在服务端）。
/// 必须在本地行 uuid 已被改成 incoming（旧 uuid 已释放）之后调用，避免 uuid 唯一冲突。
fn reap_orphan_uuid(conn: &Connection, table: &str, old_uuid: &str) -> AppResult<()> {
    let now = now_ms();
    // 墓碑 name 用占位串（被软删、partial unique 不约束；占位防与活行/其它墓碑撞）。
    let placeholder = format!("\u{1}orphan-{old_uuid}");
    let sql = match table {
        "folders" => {
            "INSERT INTO folders (uuid, name, sort_order, created_at, updated_at, deleted_at, dirty) \
             VALUES (?1, ?2, 0, ?3, ?3, ?3, 1)"
        }
        "tags" => {
            "INSERT INTO tags (uuid, name, color, created_at, updated_at, deleted_at, dirty) \
             VALUES (?1, ?2, NULL, ?3, ?3, ?3, 1)"
        }
        _ => return Ok(()),
    };
    conn.execute(sql, params![old_uuid, placeholder, now])
        .map_err(dberr)?;
    Ok(())
}

/// LWW：决定服务端来的变更是否覆盖本地。target = 本地行 (updated_at, dirty)。
fn decide_write(target: Option<(i64, i64)>, incoming_updated: i64) -> bool {
    match target {
        None => true,
        Some((local_updated, local_dirty)) => {
            let keep_local = local_dirty == 1 && local_updated > incoming_updated;
            let already_current = local_dirty == 0 && incoming_updated <= local_updated;
            !(keep_local || already_current)
        }
    }
}

/// apply 一条服务端变更（pull 后调用）。返回是否真的改了本地（用于决定 emit 哪些事件）。
/// 调用前应已按 folders/tags → prompts → sites 排序，保证 prompt 的引用能解析。
pub fn apply_change(conn: &mut Connection, ch: &Change) -> AppResult<bool> {
    match ch.entity.as_str() {
        ENTITY_FOLDER => apply_folder(conn, ch),
        ENTITY_TAG => apply_tag(conn, ch),
        ENTITY_SITE => apply_site(conn, ch),
        ENTITY_PROMPT => apply_prompt(conn, ch),
        other => Err(AppError::InvalidInput(format!("unknown entity {other}"))),
    }
}

fn apply_folder(conn: &Connection, ch: &Change) -> AppResult<bool> {
    let d: FolderData = serde_json::from_value(ch.data.clone())
        .map_err(|e| AppError::Internal(format!("folder data: {e}")))?;
    // 墓碑只按 uuid 匹配，绝不按名收编 —— 否则一台设备删除它自己的同名 folder 会被收编到
    // 另一台不相关的同名存活 folder 上，把后者误软删并孤立其 prompt（review #6）。
    let collide_name = if ch.deleted_at.is_none() {
        Some(d.name.as_str())
    } else {
        None
    };
    let target = find_target(conn, "folders", &ch.uuid, collide_name)?;
    if !decide_write(target.as_ref().map(|t| (t.1, t.2)), ch.updated_at) {
        return Ok(false);
    }
    match target {
        Some((id, _, _, old_uuid)) => {
            conn.execute(
                "UPDATE folders SET uuid=?1, name=?2, sort_order=?3, created_at=?4, \
                 updated_at=?5, deleted_at=?6, dirty=0 WHERE id=?7",
                params![
                    ch.uuid,
                    d.name,
                    d.sort_order,
                    d.created_at,
                    ch.updated_at,
                    ch.deleted_at,
                    id
                ],
            )
            .map_err(dberr)?;
            if let Some(old) = old_uuid {
                if old != ch.uuid {
                    reap_orphan_uuid(conn, "folders", &old)?;
                }
            }
        }
        None => {
            conn.execute(
                "INSERT INTO folders (uuid, name, sort_order, created_at, updated_at, deleted_at, dirty) \
                 VALUES (?1,?2,?3,?4,?5,?6,0)",
                params![ch.uuid, d.name, d.sort_order, d.created_at, ch.updated_at, ch.deleted_at],
            )
            .map_err(dberr)?;
        }
    }
    Ok(true)
}

fn apply_tag(conn: &Connection, ch: &Change) -> AppResult<bool> {
    let d: TagData = serde_json::from_value(ch.data.clone())
        .map_err(|e| AppError::Internal(format!("tag data: {e}")))?;
    // 墓碑只按 uuid 匹配（同 apply_folder，review #6）。
    let collide_name = if ch.deleted_at.is_none() {
        Some(d.name.as_str())
    } else {
        None
    };
    let target = find_target(conn, "tags", &ch.uuid, collide_name)?;
    if !decide_write(target.as_ref().map(|t| (t.1, t.2)), ch.updated_at) {
        return Ok(false);
    }
    match target {
        Some((id, _, _, old_uuid)) => {
            conn.execute(
                "UPDATE tags SET uuid=?1, name=?2, color=?3, created_at=?4, \
                 updated_at=?5, deleted_at=?6, dirty=0 WHERE id=?7",
                params![
                    ch.uuid,
                    d.name,
                    d.color,
                    d.created_at,
                    ch.updated_at,
                    ch.deleted_at,
                    id
                ],
            )
            .map_err(dberr)?;
            if let Some(old) = old_uuid {
                if old != ch.uuid {
                    reap_orphan_uuid(conn, "tags", &old)?;
                }
            }
        }
        None => {
            conn.execute(
                "INSERT INTO tags (uuid, name, color, created_at, updated_at, deleted_at, dirty) \
                 VALUES (?1,?2,?3,?4,?5,?6,0)",
                params![
                    ch.uuid,
                    d.name,
                    d.color,
                    d.created_at,
                    ch.updated_at,
                    ch.deleted_at
                ],
            )
            .map_err(dberr)?;
        }
    }
    Ok(true)
}

fn apply_site(conn: &Connection, ch: &Change) -> AppResult<bool> {
    let d: SiteData = serde_json::from_value(ch.data.clone())
        .map_err(|e| AppError::Internal(format!("site data: {e}")))?;
    let target = find_target(conn, "sites", &ch.uuid, None)?;
    if !decide_write(target.as_ref().map(|t| (t.1, t.2)), ch.updated_at) {
        return Ok(false);
    }
    match target {
        Some((id, _, _, _)) => {
            conn.execute(
                "UPDATE sites SET name=?1, url=?2, sort_order=?3, created_at=?4, \
                 updated_at=?5, deleted_at=?6, dirty=0 WHERE id=?7",
                params![
                    d.name,
                    d.url,
                    d.sort_order,
                    d.created_at,
                    ch.updated_at,
                    ch.deleted_at,
                    id
                ],
            )
            .map_err(dberr)?;
        }
        None => {
            conn.execute(
                "INSERT INTO sites (uuid, name, url, sort_order, created_at, updated_at, deleted_at, dirty) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,0)",
                params![ch.uuid, d.name, d.url, d.sort_order, d.created_at, ch.updated_at, ch.deleted_at],
            )
            .map_err(dberr)?;
        }
    }
    Ok(true)
}

fn apply_prompt(conn: &mut Connection, ch: &Change) -> AppResult<bool> {
    let d: PromptData = serde_json::from_value(ch.data.clone())
        .map_err(|e| AppError::Internal(format!("prompt data: {e}")))?;
    let target = find_target(conn, "prompts", &ch.uuid, None)?;
    if let Some((id, _, _, _)) = &target {
        let local: bool = conn
            .query_row(
                "SELECT rich_content IS NOT NULL FROM prompts WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(dberr)?;
        if local {
            return Ok(false);
        }
    }
    if !decide_write(target.as_ref().map(|t| (t.1, t.2)), ch.updated_at) {
        return Ok(false);
    }
    // 解析 folder_uuid -> 本地 folder_id（缺失则置 NULL，folder 可能本轮稍后/下轮到）。
    let folder_id: Option<i64> = match &d.folder_uuid {
        Some(fu) => conn
            .query_row("SELECT id FROM folders WHERE uuid = ?1", params![fu], |r| {
                r.get::<_, i64>(0)
            })
            .optional()
            .map_err(dberr)?,
        None => None,
    };

    let tx = conn.transaction().map_err(dberr)?;
    let pid: i64 = match target {
        Some((id, _, _, _)) => {
            tx.execute(
                "UPDATE prompts SET title=?1, content=?2, folder_id=?3, is_favorite=?4, \
                 is_pinned=?5, created_at=?6, updated_at=?7, deleted_at=?8, dirty=0 WHERE id=?9",
                params![
                    d.title,
                    d.content,
                    folder_id,
                    d.is_favorite as i64,
                    d.is_pinned as i64,
                    d.created_at,
                    ch.updated_at,
                    ch.deleted_at,
                    id
                ],
            )
            .map_err(dberr)?;
            id
        }
        None => {
            tx.execute(
                "INSERT INTO prompts (uuid, title, content, folder_id, is_favorite, is_pinned, \
                 use_count, created_at, updated_at, deleted_at, dirty) \
                 VALUES (?1,?2,?3,?4,?5,?6,0,?7,?8,?9,0)",
                params![
                    ch.uuid,
                    d.title,
                    d.content,
                    folder_id,
                    d.is_favorite as i64,
                    d.is_pinned as i64,
                    d.created_at,
                    ch.updated_at,
                    ch.deleted_at
                ],
            )
            .map_err(dberr)?;
            tx.last_insert_rowid()
        }
    };
    // 重建 junction：墓碑 prompt 不带标签。
    tx.execute("DELETE FROM prompt_tags WHERE prompt_id = ?1", params![pid])
        .map_err(dberr)?;
    if ch.deleted_at.is_none() {
        for tu in &d.tag_uuids {
            let tid: Option<i64> = tx
                .query_row("SELECT id FROM tags WHERE uuid = ?1", params![tu], |r| {
                    r.get::<_, i64>(0)
                })
                .optional()
                .map_err(dberr)?;
            if let Some(tid) = tid {
                tx.execute(
                    "INSERT OR IGNORE INTO prompt_tags (prompt_id, tag_id) VALUES (?1, ?2)",
                    params![pid, tid],
                )
                .map_err(dberr)?;
            }
        }
    }
    tx.commit().map_err(dberr)?;
    Ok(true)
}

/// 同步循环每拍调用：把当前墙钟写进 sync_state.last_sync_at（便于 UI 显示）。
pub fn touch_synced(conn: &Connection) -> AppResult<()> {
    super::sync_state::touch_last_sync(conn, now_ms())
}

/// 物理删除已确认推送(dirty=0)的旧墓碑（deleted_at 早于 now-retention_ms）。
/// 安全前提：墓碑已 push（dirty=0），且本地游标已越过它 ⇒ 服务端不会再重发同一墓碑
/// （墓碑记录不会再变，除非被 un-delete，本应用无此操作）。返回删除行数。
pub fn gc_tombstones(conn: &Connection, retention_ms: i64) -> AppResult<u64> {
    let cutoff = now_ms() - retention_ms;
    let mut total = 0u64;
    for t in ["folders", "tags", "prompts", "sites"] {
        let n = conn
            .execute(
                &format!(
                    "DELETE FROM {t} WHERE deleted_at IS NOT NULL AND dirty = 0 AND deleted_at < ?1"
                ),
                params![cutoff],
            )
            .map_err(dberr)?;
        total += n as u64;
    }
    Ok(total)
}

/// 账户切换时清空本地可同步数据（folders/tags/prompts/sites + junction）。
/// 防上一个账户的本地数据泄漏给新账户，并让新账户从 cursor=0 拉自己的数据。
/// 物理删除（非软删）。settings / clipboard 不在同步范围、不动。
pub fn wipe_syncable(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "DELETE FROM prompt_tags; \
         DELETE FROM prompts; \
         DELETE FROM tags; \
         DELETE FROM folders; \
         DELETE FROM sites;",
    )
    .map_err(dberr)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{folders, memory_conn, prompts, sync_state, tags};
    use crate::models::prompt::PromptDraft;

    fn pull_change(entity: &str, uuid: &str, updated_at: i64, data: serde_json::Value) -> Change {
        Change {
            entity: entity.into(),
            uuid: uuid.into(),
            updated_at,
            deleted_at: None,
            data,
            seq: 1,
        }
    }

    #[test]
    fn tombstone_does_not_adopt_live_same_named_row() {
        let mut c = memory_conn();
        folders::create(&c, "Work").unwrap();
        let ua: String = c
            .query_row("SELECT uuid FROM folders WHERE name = 'Work'", [], |r| {
                r.get(0)
            })
            .unwrap();
        // 服务端来一条「不同 uuid 的 'Work' 墓碑」（别的设备删了它自己的同名 folder）。
        let ch = Change {
            entity: ENTITY_FOLDER.into(),
            uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            updated_at: 9999,
            deleted_at: Some(9999),
            data: json!({"name":"Work","sort_order":0,"created_at":1}),
            seq: 1,
        };
        apply_change(&mut c, &ch).unwrap();
        // 本地存活 'Work'(uuidA) 不被误删 / 不被按名收编。
        let live = folders::list(&c).unwrap();
        assert_eq!(
            live.len(),
            1,
            "local live folder survives unrelated tombstone"
        );
        let still: String = c
            .query_row(
                "SELECT uuid FROM folders WHERE name = 'Work' AND deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(still, ua, "uuid not adopted by the tombstone");
    }

    #[test]
    fn name_adopt_reaps_old_uuid_as_tombstone() {
        let mut c = memory_conn();
        folders::create(&c, "Work").unwrap();
        // 本地 'Work' 已推送（uuidA, dirty=0）。
        c.execute(
            "UPDATE folders SET uuid='aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', dirty=0, updated_at=10 \
             WHERE name='Work'",
            [],
        )
        .unwrap();
        // 服务端来「同名 'Work' 但不同 uuidB」的存活变更（更新更晚）→ 按名收编。
        let ch = Change {
            entity: ENTITY_FOLDER.into(),
            uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            updated_at: 100,
            deleted_at: None,
            data: json!({"name":"Work","sort_order":0,"created_at":1}),
            seq: 1,
        };
        apply_change(&mut c, &ch).unwrap();
        // 活的 'Work' 现在 uuid = B。
        let live_uuid: String = c
            .query_row(
                "SELECT uuid FROM folders WHERE name='Work' AND deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(live_uuid, "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
        // 旧 uuidA 留了一条墓碑、dirty=1（待 push 让服务端回收孤儿）。
        let (del, dirty): (Option<i64>, i64) = c
            .query_row(
                "SELECT deleted_at, dirty FROM folders WHERE uuid='aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(del.is_some(), "old uuid tombstoned");
        assert_eq!(dirty, 1, "old uuid queued for reaping push");
    }

    #[test]
    fn apply_inserts_new_folder_then_lww() {
        let mut c = memory_conn();
        let u = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let ch = pull_change(
            ENTITY_FOLDER,
            u,
            100,
            json!({"name":"work","sort_order":0,"created_at":50}),
        );
        assert!(apply_change(&mut c, &ch).unwrap());
        let list = folders::list(&c).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "work");

        // 更旧的 incoming 被忽略。
        let older = pull_change(
            ENTITY_FOLDER,
            u,
            50,
            json!({"name":"OLD","sort_order":0,"created_at":50}),
        );
        assert!(!apply_change(&mut c, &older).unwrap());
        assert_eq!(folders::list(&c).unwrap()[0].name, "work");

        // 更新的 incoming 覆盖。
        let newer = pull_change(
            ENTITY_FOLDER,
            u,
            200,
            json!({"name":"WORK2","sort_order":1,"created_at":50}),
        );
        assert!(apply_change(&mut c, &newer).unwrap());
        assert_eq!(folders::list(&c).unwrap()[0].name, "WORK2");
    }

    #[test]
    fn local_dirty_newer_wins_over_incoming() {
        let mut c = memory_conn();
        // 本地建一个 folder（dirty=1, updated_at=now 较大）。
        let f = folders::create(&c, "mine").unwrap();
        let local_uuid: String = c
            .query_row("SELECT uuid FROM folders WHERE id=?1", [f.id], |r| r.get(0))
            .unwrap();
        // 服务端来的同 uuid 但更旧 → 本地脏且更新 → 保留本地。
        let incoming = pull_change(
            ENTITY_FOLDER,
            &local_uuid,
            1, // 远小于本地 now
            json!({"name":"server","sort_order":9,"created_at":1}),
        );
        assert!(!apply_change(&mut c, &incoming).unwrap());
        assert_eq!(folders::list(&c).unwrap()[0].name, "mine");
    }

    #[test]
    fn apply_prompt_resolves_folder_and_tag_uuids() {
        let mut c = memory_conn();
        let fu = "ffffffff-ffff-4fff-8fff-ffffffffffff";
        let tu = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
        apply_change(
            &mut c,
            &pull_change(
                ENTITY_FOLDER,
                fu,
                100,
                json!({"name":"F","sort_order":0,"created_at":1}),
            ),
        )
        .unwrap();
        apply_change(
            &mut c,
            &pull_change(
                ENTITY_TAG,
                tu,
                100,
                json!({"name":"T","color":null,"created_at":1}),
            ),
        )
        .unwrap();
        let pu = "11111111-1111-4111-8111-111111111111";
        apply_change(
            &mut c,
            &pull_change(
                ENTITY_PROMPT,
                pu,
                100,
                json!({
                    "title":"hi","content":"b","is_favorite":false,"is_pinned":false,
                    "created_at":1,"folder_uuid":fu,"tag_uuids":[tu]
                }),
            ),
        )
        .unwrap();
        let list = prompts::list(&c, crate::models::prompt::SortMode::Created).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "hi");
        assert!(list[0].folder_id.is_some(), "folder_uuid resolved");
        assert_eq!(list[0].tag_ids.len(), 1, "tag_uuid resolved");
    }

    #[test]
    fn collect_dirty_round_trips_prompt_refs() {
        let mut c = memory_conn();
        let t = tags::create(&c, "x", None).unwrap().id;
        let f = folders::create(&c, "fold").unwrap().id;
        prompts::create(
            &mut c,
            PromptDraft {
                rich_content: None,
                title: "p".into(),
                content: "b".into(),
                folder_id: Some(f),
                tag_ids: vec![t],
            },
        )
        .unwrap();
        let dirty = collect_dirty(&c).unwrap();
        // folder + tag + prompt 都应在 dirty 集合里（新建即 dirty）。
        let prompt = dirty
            .iter()
            .find(|c| c.entity == ENTITY_PROMPT)
            .expect("prompt in dirty");
        let pd: PromptData = serde_json::from_value(prompt.data.clone()).unwrap();
        assert!(pd.folder_uuid.is_some());
        assert_eq!(pd.tag_uuids.len(), 1);
    }

    #[test]
    fn clear_dirty_and_count() {
        let c = memory_conn();
        folders::create(&c, "a").unwrap();
        assert_eq!(dirty_count(&c).unwrap(), 1);
        let (u, upd): (String, i64) = c
            .query_row("SELECT uuid, updated_at FROM folders LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        // 版本不符不清（守卫）。
        clear_dirty(&c, ENTITY_FOLDER, &u, upd + 999).unwrap();
        assert_eq!(
            dirty_count(&c).unwrap(),
            1,
            "stale-version clear is a no-op"
        );
        // 版本相符才清。
        clear_dirty(&c, ENTITY_FOLDER, &u, upd).unwrap();
        assert_eq!(dirty_count(&c).unwrap(), 0);
        let _ = sync_state::get(&c).unwrap();
    }
}
