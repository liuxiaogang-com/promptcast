// commands/update.rs — 应用更新：查更新 / 下载并安装 / 读写更新清单地址。
// 清单托管在外部（CNB），地址存 settings 原始 key，可在设置页改。
//
// 关键：本应用是单 SQLite 连接（DbState 一把锁）。查更新/下载会有数十秒~数分钟网络 IO，
// 绝不能持锁做网络 —— 否则整库冻结（剪贴板监听、同步、列表全卡）。这里统一“短锁取地址 →
// 释放 → 再走网络”。
use tauri::{AppHandle, State};

use crate::db::DbState;
use crate::error::{AppError, AppResult};
use crate::update::{self, ReleaseNotes, UpdateInfo};

fn map_err(e: update::UpdateError) -> AppError {
    AppError::Internal(e.to_string())
}

/// 取清单地址（短锁），空串表示未配置。
fn resolve_url(db: &State<'_, DbState>) -> String {
    let conn = db.0.lock();
    update::get_manifest_url(&conn)
}

/// 查更新。未配置 / 已是最新 → 返回 null；本平台无包返回明确错误。
/// `(async)`：同步命令跑在主线程，阻塞的 HTTP 拉清单会卡死 UI；标记后 Tauri 放到独立线程执行。
#[tauri::command(async)]
pub fn update_check(db: State<'_, DbState>) -> AppResult<Option<UpdateInfo>> {
    let url = resolve_url(&db);
    if url.is_empty() {
        // 未配置地址不算错误：静默当作“无更新”，避免启动自动查时弹错。
        return Ok(None);
    }
    update::check(&url).map_err(map_err)
}

/// 下载本平台安装包（带进度事件 update-progress）并拉起安装器。
/// 不接收前端传的 url —— 后端按当前平台重新读清单，避免被骗下载任意二进制。
/// `(async)`：必须在独立线程跑，否则数秒~数分钟的阻塞下载会卡死主线程（鼠标转圈/假死）。
#[tauri::command(async)]
pub fn update_download_install(
    app: AppHandle,
    db: State<'_, DbState>,
    expected_version: String,
) -> AppResult<()> {
    let url = resolve_url(&db);
    if url.is_empty() {
        return Err(map_err(update::UpdateError::NotConfigured));
    }
    update::download_and_install(&app, &url, &expected_version).map_err(map_err)
}

#[tauri::command]
pub fn update_get_manifest_url(db: State<'_, DbState>) -> AppResult<String> {
    Ok(resolve_url(&db))
}

#[tauri::command]
pub fn update_set_manifest_url(db: State<'_, DbState>, url: String) -> AppResult<()> {
    let conn = db.0.lock();
    update::set_manifest_url(&conn, &url)
}

#[tauri::command(async)]
pub fn update_changelog() -> AppResult<Vec<ReleaseNotes>> {
    update::changelog().map_err(map_err)
}
