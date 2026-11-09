// commands/sync.rs — 同步控制：状态 / 开关 / 服务器地址 / 立即同步。
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Manager, State};

use crate::db::{sync_state, DbState};
use crate::error::{AppError, AppResult};
use crate::sync::{self, SyncRuntime, SyncStatus};

#[tauri::command]
pub fn sync_status(app: AppHandle) -> AppResult<SyncStatus> {
    Ok(sync::current_status(&app, "idle", None))
}

#[tauri::command]
pub fn sync_set_enabled(
    app: AppHandle,
    db: State<'_, DbState>,
    rt: State<'_, SyncRuntime>,
    enabled: bool,
) -> AppResult<()> {
    {
        let conn = db.0.lock();
        sync_state::set_enabled(&conn, enabled)?;
    }
    if enabled {
        rt.wake.store(true, Ordering::Relaxed);
    }
    sync::emit_status(&app, "idle", None);
    Ok(())
}

#[tauri::command]
pub fn sync_now(rt: State<'_, SyncRuntime>) -> AppResult<()> {
    rt.wake.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn sync_get_server_url(db: State<'_, DbState>) -> AppResult<String> {
    let conn = db.0.lock();
    Ok(sync::server_url(&conn))
}

#[tauri::command]
pub fn sync_set_server_url(app: AppHandle, db: State<'_, DbState>, url: String) -> AppResult<()> {
    let rt = app.state::<SyncRuntime>();
    let _guard = rt
        .account_gate
        .try_lock()
        .ok_or_else(|| AppError::Internal("正在同步或处理账户请求，请稍后重试".into()))?;
    let conn = db.0.lock();
    if sync::server_url(&conn).trim_end_matches('/') == url.trim().trim_end_matches('/') {
        return Ok(());
    }
    if sync_state::get(&conn)?.user_id.is_some() {
        return Err(AppError::InvalidInput(
            "本地数据已绑定同步账户，不能直接更换服务器，以免数据串户".into(),
        ));
    }
    sync::set_server_url(&conn, &url)
}
