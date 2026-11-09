// sync/engine.rs — 一次同步周期：push dirty → pull 增量 → apply（LWW）→ 推进游标 → emit 事件。
use std::collections::HashSet;

use tauri::{AppHandle, Manager};

use super::client::{Client, SyncError};
use super::{clear_refresh, load_refresh, server_url, store_refresh, SyncRuntime};
use crate::db::{sync_repo, sync_state, DbState};
use crate::error::{AppError, AppResult};
use crate::events;
use crate::models::sync::{ENTITY_FOLDER, ENTITY_PROMPT, ENTITY_SITE, ENTITY_TAG};

const PULL_LIMIT: i64 = 500;
const MAX_PAGES: usize = 50;

#[derive(Debug, Default)]
pub struct Outcome {
    pub pushed: usize,
    pub pulled: usize,
}

/// 跑一次完整同步。未登录 / 未开启 → 直接返回（Outcome 全 0）。
/// 网络/401 等错误向上抛；调用方（sync_loop）记录并下拍重试。
pub fn sync_once(app: &AppHandle) -> AppResult<Outcome> {
    let db = app.state::<DbState>();
    let rt = app.state::<SyncRuntime>();
    let Some(_account_guard) = rt.account_gate.try_lock() else {
        return Ok(Outcome::default());
    };

    let (enabled, user_id, mut cursor, base) = {
        let conn = db.0.lock();
        if crate::db::settings::get_raw(&conn, super::AUTH_TRANSITION_KEY)?.as_deref() == Some("1")
        {
            return Err(AppError::Internal(
                "账户变更未完成，请重新登录；本地数据已保留".into(),
            ));
        }
        let s = sync_state::get(&conn)?;
        let base = server_url(&conn);
        (s.sync_enabled, s.user_id, s.last_pull_cursor, base)
    };
    if !enabled || user_id.is_none() {
        return Ok(Outcome::default());
    }

    let client = Client::new(&base);
    // 先释放锁再进 None 分支：refresh_access 会再次锁 rt.access，parking_lot 不可重入，
    // 否则首次同步（重启后 access 为 None、仅 keyring 有 refresh）会自死锁卡死同步线程。
    let cached_access = rt.access.lock().clone();
    let mut access = match cached_access {
        Some(a) => a,
        None => refresh_access(&client, &rt)?,
    };

    // ---- PUSH ----
    let dirty = {
        let conn = db.0.lock();
        sync_repo::collect_dirty(&conn)?
    };
    let mut pushed = 0usize;
    if !dirty.is_empty() {
        let resp = match client.push(&access, &dirty) {
            Ok(r) => r,
            Err(SyncError::Unauthorized) => {
                access = refresh_access(&client, &rt)?;
                client.push(&access, &dirty).map_err(to_apperr)?
            }
            Err(e) => return Err(to_apperr(e)),
        };
        let conn = db.0.lock();
        for item in &resp.results {
            if item.applied {
                if let Some(ch) = dirty.iter().find(|c| c.uuid == item.uuid) {
                    sync_repo::clear_dirty(&conn, &ch.entity, &item.uuid, ch.updated_at)?;
                    pushed += 1;
                }
            }
        }
    }

    // ---- PULL ----
    // 全部翻页先收进 buffer，再全局排序后 apply：保证 folders/tags 全局先于 prompts，
    // 即使一个 prompt 与它的 folder 落在不同分页也能解析引用（修跨页乱序）。
    let mut pulled = 0usize;
    let mut changed: HashSet<String> = HashSet::new();
    let mut buffer: Vec<crate::models::sync::Change> = Vec::new();
    let mut pages = 0usize;
    loop {
        if pages >= MAX_PAGES {
            tracing::warn!(pages, "pull hit MAX_PAGES cap; continuing next cycle");
            break;
        }
        let resp = match client.pull(&access, cursor, PULL_LIMIT) {
            Ok(r) => r,
            Err(SyncError::Unauthorized) => {
                access = refresh_access(&client, &rt)?;
                client
                    .pull(&access, cursor, PULL_LIMIT)
                    .map_err(to_apperr)?
            }
            Err(e) => return Err(to_apperr(e)),
        };
        if resp.changes.is_empty() {
            break;
        }
        cursor = resp.next_cursor;
        let has_more = resp.has_more;
        buffer.extend(resp.changes);
        pages += 1;
        if !has_more {
            break;
        }
    }
    if !buffer.is_empty() {
        buffer.sort_by(|a, b| {
            entity_order(&a.entity)
                .cmp(&entity_order(&b.entity))
                .then(a.seq.cmp(&b.seq))
        });
        let mut conn = db.0.lock();
        for ch in &buffer {
            // 单条 apply 失败（坏数据 / 偶发约束冲突）只记日志 + 跳过，绝不让一条毒记录
            // 卡死整条同步流 —— cursor 照常前进。
            match sync_repo::apply_change(&mut conn, ch) {
                Ok(true) => {
                    changed.insert(ch.entity.clone());
                    pulled += 1;
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(entity = %ch.entity, uuid = %ch.uuid, error = %e, "apply_change failed; skipping record");
                }
            }
        }
        sync_state::set_cursor(&conn, cursor)?;
    }

    {
        let conn = db.0.lock();
        sync_repo::touch_synced(&conn)?;
        // 清理已推送(dirty=0)的旧墓碑（保留 30 天），防止本地无限增长。
        let _ = sync_repo::gc_tombstones(&conn, 30 * 24 * 3600 * 1000);
    }

    if changed.contains(ENTITY_FOLDER) {
        events::emit_folders_changed(app);
    }
    if changed.contains(ENTITY_TAG) {
        events::emit_tags_changed(app);
    }
    if changed.contains(ENTITY_PROMPT) {
        events::emit_prompts_changed(app);
    }
    if changed.contains(ENTITY_SITE) {
        events::emit_sites_changed(app);
        // favicon 不走同步：拉到新/变更站点后，后台给缺图标的站点自取。
        crate::commands::sites::refetch_missing_favicons(app.clone());
    }

    Ok(Outcome { pushed, pulled })
}

fn entity_order(entity: &str) -> u8 {
    match entity {
        ENTITY_FOLDER => 0,
        ENTITY_TAG => 1,
        ENTITY_PROMPT => 2,
        ENTITY_SITE => 3,
        _ => 4,
    }
}

/// 用 keyring 里的 refresh token 换新的 access（并轮换 refresh）。失败视为登出。
fn refresh_access(client: &Client, rt: &SyncRuntime) -> AppResult<String> {
    let refresh = load_refresh().ok_or_else(|| AppError::Internal("not logged in".into()))?;
    match client.refresh(&refresh) {
        Ok(t) => {
            store_refresh(&t.refresh);
            *rt.access.lock() = Some(t.access.clone());
            Ok(t.access)
        }
        Err(e) => {
            // 只有「确实无效」(401) 才清 refresh token（真登出）；网络/5xx/超时保留 token、
            // 下拍重试，避免一次网络抖动把用户误登出（review #9）。
            if matches!(e, SyncError::Unauthorized) {
                clear_refresh();
                *rt.access.lock() = None;
            }
            Err(AppError::Internal(format!("refresh failed: {e}")))
        }
    }
}

fn to_apperr(e: SyncError) -> AppError {
    AppError::Internal(format!("sync: {e}"))
}
