// commands/auth.rs — 账户（同步用）：注册 / 登录 / 登出 / 状态。
// access token 存内存(SyncRuntime)；refresh token 存系统钥匙串；email 存 settings 原始 key
// 便于重启后 UI 显示。登录成功即开启同步并触发一拍。
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Manager, State};

use crate::db::{settings, sync_repo, sync_state, DbState};
use crate::error::{AppError, AppResult};
use crate::sync::{
    self,
    client::{Client, SyncError},
    SyncRuntime,
};

const USER_EMAIL_KEY: &str = "sync_user_email";

async fn run_auth<T: Send + 'static>(
    app: AppHandle,
    task: impl FnOnce(&AppHandle) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let rt = app.state::<SyncRuntime>();
        let _guard = rt
            .account_gate
            .try_lock()
            .ok_or_else(|| AppError::Internal("正在同步或处理账户请求，请稍后重试".into()))?;
        task(&app)
    })
    .await
    .map_err(|e| AppError::Internal(format!("账户任务执行失败：{e}")))?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuthStatus {
    pub logged_in: bool,
    pub email: Option<String>,
}

fn client_for(db: &DbState) -> Client {
    let conn = db.0.lock();
    Client::new(&sync::server_url(&conn))
}

fn on_authed(
    app: &AppHandle,
    db: &DbState,
    rt: &SyncRuntime,
    user_id: &str,
    email: &str,
    access: String,
    refresh: String,
) -> AppResult<()> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let previous = sync_state::get(&db.0.lock())?.user_id;
    let switching = previous.as_deref().is_some_and(|id| id != user_id);
    if switching {
        let mut dialog = app.dialog()
            .message("切换后将清空旧账户在本机的提示词、分类、标签和网址，包括尚未同步的内容。云端数据、剪贴板历史和应用设置不受影响。需要保留的内容请先取消并导出备份。")
            .title("切换账户并清空本地内容？")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom("清空并切换".into(), "取消".into()));
        if let Some(window) = app.get_webview_window("settings") {
            dialog = dialog.parent(&window);
        }
        if !dialog.blocking_show() {
            return Err(AppError::InvalidInput(
                "已取消账户切换，本地数据未修改".into(),
            ));
        }
    }
    {
        let mut conn = db.0.lock();
        if sync_state::get(&conn)?.user_id != previous {
            return Err(AppError::Internal("账户状态已变化，请重新登录".into()));
        }
        commit_login(&mut conn, user_id, email, || {
            if sync::store_refresh(&refresh) {
                Ok(())
            } else {
                Err(AppError::Internal(
                    "无法保存登录凭据，本地数据未修改，请重新登录".into(),
                ))
            }
        })?;
    }
    *rt.access.lock() = Some(access);
    *rt.email.lock() = Some(email.to_string());
    if switching {
        crate::events::emit_prompts_changed(app);
        crate::events::emit_folders_changed(app);
        crate::events::emit_tags_changed(app);
        crate::events::emit_sites_changed(app);
    }
    rt.wake.store(true, Ordering::Relaxed);
    sync::emit_status(app, "idle", None);
    Ok(())
}

// 先持久化过渡标记：凭据写入与提交之间崩溃时必须重登，避免跨账户同步。
fn commit_login(
    conn: &mut rusqlite::Connection,
    user_id: &str,
    email: &str,
    save_credentials: impl FnOnce() -> AppResult<()>,
) -> AppResult<()> {
    settings::set_raw(conn, sync::AUTH_TRANSITION_KEY, "1")?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let previous = sync_state::get(&tx)?.user_id;
    if previous.as_deref().is_some_and(|id| id != user_id) {
        sync_repo::wipe_syncable(&tx)?;
        sync_state::set_cursor(&tx, 0)?;
    }
    sync_state::set_user(&tx, Some(user_id))?;
    sync_state::set_enabled(&tx, true)?;
    settings::set_raw(&tx, USER_EMAIL_KEY, email)?;
    settings::set_raw(&tx, sync::AUTH_TRANSITION_KEY, "0")?;
    save_credentials()?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))
}

fn auth_register_blocking(
    app: AppHandle,
    db: State<'_, DbState>,
    rt: State<'_, SyncRuntime>,
    email: String,
    password: String,
) -> AppResult<AuthStatus> {
    let r = client_for(&db)
        .register(&email, &password)
        .map_err(|e| AppError::Internal(format!("注册失败: {e}")))?;
    on_authed(&app, &db, &rt, &r.user_id, &email, r.access, r.refresh)?;
    tracing::info!("auth: registered + logged in");
    Ok(AuthStatus {
        logged_in: true,
        email: Some(email),
    })
}

fn auth_login_blocking(
    app: AppHandle,
    db: State<'_, DbState>,
    rt: State<'_, SyncRuntime>,
    email: String,
    password: String,
) -> AppResult<AuthStatus> {
    let r = client_for(&db)
        .login(&email, &password)
        .map_err(|e| AppError::Internal(format!("登录失败: {e}")))?;
    on_authed(&app, &db, &rt, &r.user_id, &email, r.access, r.refresh)?;
    tracing::info!("auth: logged in");
    Ok(AuthStatus {
        logged_in: true,
        email: Some(email),
    })
}

fn auth_logout_blocking(
    app: AppHandle,
    db: State<'_, DbState>,
    rt: State<'_, SyncRuntime>,
) -> AppResult<()> {
    // best-effort 撤销服务端 refresh，再清本地会话。
    if let Some(refresh) = sync::load_refresh() {
        let _ = client_for(&db).logout(&refresh);
    }
    {
        let mut conn = db.0.lock();
        settings::set_raw(&conn, sync::AUTH_TRANSITION_KEY, "1")?;
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Db(e.to_string()))?;
        sync_state::set_enabled(&tx, false)?;
        settings::set_raw(&tx, USER_EMAIL_KEY, "")?;
        settings::set_raw(&tx, sync::AUTH_TRANSITION_KEY, "0")?;
        if !sync::clear_refresh() {
            return Err(AppError::Internal(
                "清除登录凭据失败，已暂停同步，请重试退出".into(),
            ));
        }
        tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    }
    *rt.access.lock() = None;
    *rt.email.lock() = None;
    sync::emit_status(&app, "logged_out", None);
    tracing::info!("auth: logged out");
    Ok(())
}

#[tauri::command]
pub fn auth_status(db: State<'_, DbState>, rt: State<'_, SyncRuntime>) -> AppResult<AuthStatus> {
    if settings::get_raw(&db.0.lock(), sync::AUTH_TRANSITION_KEY)?.as_deref() == Some("1") {
        return Ok(AuthStatus {
            logged_in: false,
            email: None,
        });
    }
    let email = rt.email.lock().clone();
    let logged_in = email.is_some() || sync::load_refresh().is_some();
    Ok(AuthStatus { logged_in, email })
}

fn map_sync_err(e: SyncError) -> AppError {
    AppError::Internal(e.to_string())
}

/// 用 keyring 的 refresh 换新的 access（写回内存）。失败=会话过期。
fn refresh_access(client: &Client, rt: &SyncRuntime) -> AppResult<String> {
    let refresh = sync::load_refresh().ok_or_else(|| AppError::Internal("未登录".into()))?;
    let t = client
        .refresh(&refresh)
        .map_err(|e| AppError::Internal(format!("会话过期，请重新登录: {e}")))?;
    sync::store_refresh(&t.refresh);
    *rt.access.lock() = Some(t.access.clone());
    Ok(t.access)
}

/// 取一个内存 access，没有就用 keyring refresh 换。
fn access_or_refresh(client: &Client, rt: &SyncRuntime, db: &DbState) -> AppResult<String> {
    if settings::get_raw(&db.0.lock(), sync::AUTH_TRANSITION_KEY)?.as_deref() == Some("1") {
        return Err(AppError::Internal("账户变更未完成，请先重新登录".into()));
    }
    // 先把锁释放再进 None 分支 —— refresh_access 会再次锁 rt.access；parking_lot 不可重入，
    // 若把 lock() 放在 match 头部，guard 会活到整个 match 结束 → 自死锁（重启后首次必现）。
    let cached = rt.access.lock().clone();
    match cached {
        Some(a) => Ok(a),
        None => refresh_access(client, rt),
    }
}

fn auth_change_password_blocking(
    app: AppHandle,
    db: State<'_, DbState>,
    rt: State<'_, SyncRuntime>,
    old_password: String,
    new_password: String,
) -> AppResult<()> {
    let client = client_for(&db);
    let access = access_or_refresh(&client, &rt, &db)?;
    // 401 可能是 token 过期、也可能是旧密码不对；过期就刷新重试一次，旧密码错则二次也会 401 报错。
    match client.change_password(&access, &old_password, &new_password) {
        Ok(()) => {}
        Err(SyncError::Unauthorized) => {
            let a = refresh_access(&client, &rt)?;
            client
                .change_password(&a, &old_password, &new_password)
                .map_err(map_sync_err)?;
        }
        Err(e) => return Err(map_sync_err(e)),
    }
    // 改密后服务端吊销了所有 refresh（含本机）→ 用新密码重登拿新令牌，保持本机在线。
    let email = rt.email.lock().clone().or_else(|| {
        let conn = db.0.lock();
        crate::db::settings::get_raw(&conn, USER_EMAIL_KEY)
            .ok()
            .flatten()
            .filter(|e| !e.is_empty())
    });
    let relogged = match email {
        Some(email) => match client.login(&email, &new_password) {
            Ok(r) => {
                sync::store_refresh(&r.refresh);
                *rt.access.lock() = Some(r.access);
                true
            }
            Err(_) => false,
        },
        None => false,
    };
    if !relogged {
        // keyring 里的 refresh 已被服务端吊销 → 主动清会话并提示重登，避免下次刷新时静默掉线。
        sync::clear_refresh();
        *rt.access.lock() = None;
        sync::emit_status(
            &app,
            "logged_out",
            Some("密码已修改，请用新密码重新登录".into()),
        );
        tracing::warn!("auth: password changed but re-login failed; session cleared");
    } else {
        tracing::info!("auth: password changed");
    }
    Ok(())
}

fn auth_delete_account_blocking(
    app: AppHandle,
    db: State<'_, DbState>,
    rt: State<'_, SyncRuntime>,
    password: String,
) -> AppResult<()> {
    let client = client_for(&db);
    let access = access_or_refresh(&client, &rt, &db)?;
    match client.delete_account(&access, &password) {
        Ok(()) => {}
        Err(SyncError::Unauthorized) => {
            let a = refresh_access(&client, &rt)?;
            client.delete_account(&a, &password).map_err(map_sync_err)?;
        }
        Err(e) => return Err(map_sync_err(e)),
    }
    // 账户已删 → 清会话 + 清本地可同步数据（这些是已删账户的数据）+ 复位游标/用户。
    {
        let mut conn = db.0.lock();
        settings::set_raw(&conn, sync::AUTH_TRANSITION_KEY, "1")?;
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Db(e.to_string()))?;
        sync_repo::wipe_syncable(&tx)?;
        sync_state::set_cursor(&tx, 0)?;
        sync_state::set_user(&tx, None)?;
        sync_state::set_enabled(&tx, false)?;
        settings::set_raw(&tx, USER_EMAIL_KEY, "")?;
        settings::set_raw(&tx, sync::AUTH_TRANSITION_KEY, "0")?;
        if !sync::clear_refresh() {
            return Err(AppError::Internal(
                "云端账户已删除，但本地清理失败；本地数据已保留".into(),
            ));
        }
        tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    }
    *rt.access.lock() = None;
    *rt.email.lock() = None;
    crate::events::emit_prompts_changed(&app);
    crate::events::emit_folders_changed(&app);
    crate::events::emit_tags_changed(&app);
    crate::events::emit_sites_changed(&app);
    sync::emit_status(&app, "logged_out", None);
    tracing::info!("auth: account deleted + local wiped");
    Ok(())
}

/// 找回密码：返回 dev token（本地开发回显，便于测试）；生产为空。
fn auth_forgot_password_blocking(
    db: State<'_, DbState>,
    email: String,
) -> AppResult<Option<String>> {
    let r = client_for(&db)
        .forgot_password(&email)
        .map_err(map_sync_err)?;
    Ok(r.dev_token)
}

fn auth_reset_password_blocking(
    db: State<'_, DbState>,
    token: String,
    new_password: String,
) -> AppResult<()> {
    client_for(&db)
        .reset_password(&token, &new_password)
        .map_err(map_sync_err)
}

#[tauri::command]
pub async fn auth_register(
    app: AppHandle,
    email: String,
    password: String,
) -> AppResult<AuthStatus> {
    run_auth(app, move |app| {
        auth_register_blocking(
            app.clone(),
            app.state::<DbState>(),
            app.state::<SyncRuntime>(),
            email,
            password,
        )
    })
    .await
}

#[tauri::command]
pub async fn auth_login(app: AppHandle, email: String, password: String) -> AppResult<AuthStatus> {
    run_auth(app, move |app| {
        auth_login_blocking(
            app.clone(),
            app.state::<DbState>(),
            app.state::<SyncRuntime>(),
            email,
            password,
        )
    })
    .await
}

#[tauri::command]
pub async fn auth_logout(app: AppHandle) -> AppResult<()> {
    run_auth(app, move |app| {
        auth_logout_blocking(
            app.clone(),
            app.state::<DbState>(),
            app.state::<SyncRuntime>(),
        )
    })
    .await
}

#[tauri::command]
pub async fn auth_change_password(
    app: AppHandle,
    old_password: String,
    new_password: String,
) -> AppResult<()> {
    run_auth(app, move |app| {
        auth_change_password_blocking(
            app.clone(),
            app.state::<DbState>(),
            app.state::<SyncRuntime>(),
            old_password,
            new_password,
        )
    })
    .await
}

#[tauri::command]
pub async fn auth_delete_account(app: AppHandle, password: String) -> AppResult<()> {
    run_auth(app, move |app| {
        auth_delete_account_blocking(
            app.clone(),
            app.state::<DbState>(),
            app.state::<SyncRuntime>(),
            password,
        )
    })
    .await
}

#[tauri::command]
pub async fn auth_forgot_password(app: AppHandle, email: String) -> AppResult<Option<String>> {
    run_auth(app, move |app| {
        auth_forgot_password_blocking(app.state::<DbState>(), email)
    })
    .await
}

#[tauri::command]
pub async fn auth_reset_password(
    app: AppHandle,
    token: String,
    new_password: String,
) -> AppResult<()> {
    run_auth(app, move |app| {
        auth_reset_password_blocking(app.state::<DbState>(), token, new_password)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account_db() -> rusqlite::Connection {
        let conn = crate::db::memory_conn();
        conn.execute(
            "INSERT INTO folders(name, sort_order, created_at) VALUES('unsynced',0,0)",
            [],
        )
        .unwrap();
        sync_state::set_user(&conn, Some("old-user")).unwrap();
        sync_state::set_cursor(&conn, 42).unwrap();
        conn
    }

    #[test]
    fn failed_account_transition_preserves_data_and_blocks_sync() {
        for database_failure in [false, true] {
            let mut conn = account_db();
            if database_failure {
                conn.execute_batch(
                    "CREATE TRIGGER refuse_user BEFORE UPDATE ON sync_state
                    WHEN NEW.user_id='new-user' BEGIN SELECT RAISE(ABORT, 'write failure'); END;",
                )
                .unwrap();
            }
            let result = commit_login(&mut conn, "new-user", "new@example.com", || {
                Err(AppError::Internal("credential write failed".into()))
            });
            assert!(result.is_err());
            assert_eq!(
                sync_state::get(&conn).unwrap().user_id.as_deref(),
                Some("old-user")
            );
            assert_eq!(sync_state::get(&conn).unwrap().last_pull_cursor, 42);
            assert_eq!(
                conn.query_row("SELECT name FROM folders", [], |r| r.get::<_, String>(0))
                    .unwrap(),
                "unsynced"
            );
            assert_eq!(
                settings::get_raw(&conn, sync::AUTH_TRANSITION_KEY)
                    .unwrap()
                    .as_deref(),
                Some("1")
            );
        }
    }

    #[test]
    fn confirmed_account_transition_commits_data_and_identity_together() {
        let mut conn = account_db();
        commit_login(&mut conn, "new-user", "new@example.com", || Ok(())).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            sync_state::get(&conn).unwrap().user_id.as_deref(),
            Some("new-user")
        );
        assert_eq!(sync_state::get(&conn).unwrap().last_pull_cursor, 0);
        assert_eq!(
            settings::get_raw(&conn, sync::AUTH_TRANSITION_KEY)
                .unwrap()
                .as_deref(),
            Some("0")
        );
    }
}
