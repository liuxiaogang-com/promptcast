// sync — 多设备同步引擎（客户端）。离线优先：本地 SQLite 是真相源，这里只在登录+在线时
// 把 dirty 行 push 上去、把服务端增量 pull 下来 apply（LWW）。
pub mod client;
pub mod engine;

use std::sync::atomic::AtomicBool;

use parking_lot::Mutex;
use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::db::{sync_repo, sync_state, DbState};

const KEYRING_SERVICE: &str = "com.xiao.promptmanager";
const KEYRING_ACCOUNT: &str = "refresh_token";
const DEFAULT_SERVER: &str = "http://localhost:3000";
const SERVER_URL_KEY: &str = "sync_server_url";
pub const AUTH_TRANSITION_KEY: &str = "sync_auth_transition";

/// 同步运行期状态（内存）。access token 不落盘；refresh token 进系统钥匙串。
#[derive(Default)]
pub struct SyncRuntime {
    /// 串行化账户变更与完整同步周期，防止旧请求写入新账户状态。
    pub account_gate: Mutex<()>,
    pub access: Mutex<Option<String>>,
    pub email: Mutex<Option<String>>,
    /// 置位以触发一次同步（本地写后 / 获焦时）。
    pub wake: AtomicBool,
    /// 一次同步进行中。
    pub busy: AtomicBool,
}

// ---- 系统钥匙串：refresh token ----
pub fn store_refresh(token: &str) -> bool {
    match keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        Ok(e) => e.set_password(token).is_ok(),
        Err(_) => false,
    }
}
pub fn load_refresh() -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .ok()?
        .get_password()
        .ok()
}
pub fn clear_refresh() -> bool {
    match keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        Ok(e) => match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => true,
            Err(e) => {
                tracing::error!(error = %e, "failed to clear refresh credential");
                false
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "failed to open refresh credential");
            false
        }
    }
}

// ---- 服务器地址（存 settings 原始 key，不进强类型 Settings）----
pub fn server_url(conn: &Connection) -> String {
    crate::db::settings::get_raw(conn, SERVER_URL_KEY)
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SERVER.to_string())
}
pub fn set_server_url(conn: &Connection, url: &str) -> crate::error::AppResult<()> {
    crate::db::settings::set_raw(conn, SERVER_URL_KEY, url.trim())
}

// ---- 状态广播 ----
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SyncStatus {
    pub state: String, // idle | syncing | error | offline | logged_out
    pub logged_in: bool,
    pub enabled: bool,
    pub email: Option<String>,
    pub last_sync_at: Option<i64>,
    pub pending: i64,
    pub message: Option<String>,
}

pub fn current_status(app: &AppHandle, state: &str, message: Option<String>) -> SyncStatus {
    let rt = app.state::<SyncRuntime>();
    let email = rt.email.lock().clone();
    let logged_in = rt.access.lock().is_some() || email.is_some();
    let (last_sync_at, pending, enabled) = match app.try_state::<DbState>() {
        Some(db) => {
            let conn = db.0.lock();
            let s = sync_state::get(&conn).ok();
            let ls = s.as_ref().and_then(|x| x.last_sync_at);
            let enabled = s.as_ref().map(|x| x.sync_enabled).unwrap_or(false);
            let pending = sync_repo::dirty_count(&conn).unwrap_or(0);
            (ls, pending, enabled)
        }
        None => (None, 0, false),
    };
    SyncStatus {
        state: state.to_string(),
        logged_in,
        enabled,
        email,
        last_sync_at,
        pending,
        message,
    }
}

pub fn emit_status(app: &AppHandle, state: &str, message: Option<String>) {
    let s = current_status(app, state, message);
    crate::events::emit_sync_status_changed(app, s);
}
