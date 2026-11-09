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

// reqwest::blocking 会创建和销毁自己的运行时，必须整个操作都在阻塞线程中执行。
async fn run_blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, update::UpdateError> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|e| AppError::Internal(format!("更新任务执行失败：{e}")))?
        .map_err(map_err)
}

/// 取清单地址（短锁），空串表示未配置。
fn resolve_url(db: &State<'_, DbState>) -> String {
    let conn = db.0.lock();
    update::get_manifest_url(&conn)
}

/// 查更新。未配置 / 已是最新 → 返回 null；本平台无包返回明确错误。
#[tauri::command]
pub async fn update_check(db: State<'_, DbState>) -> AppResult<Option<UpdateInfo>> {
    let url = resolve_url(&db);
    if url.is_empty() {
        // 未配置地址不算错误：静默当作“无更新”，避免启动自动查时弹错。
        return Ok(None);
    }
    run_blocking(move || update::check(&url)).await
}

/// 下载本平台安装包（带进度事件 update-progress）并拉起安装器。
/// 不接收前端传的 url —— 后端按当前平台重新读清单，避免被骗下载任意二进制。
#[tauri::command]
pub async fn update_download_install(
    app: AppHandle,
    db: State<'_, DbState>,
    expected_version: String,
) -> AppResult<()> {
    let url = resolve_url(&db);
    if url.is_empty() {
        return Err(map_err(update::UpdateError::NotConfigured));
    }
    run_blocking(move || update::download_and_install(&app, &url, &expected_version)).await
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

#[tauri::command]
pub async fn update_changelog() -> AppResult<Vec<ReleaseNotes>> {
    run_blocking(update::changelog).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocking_client_can_be_created_and_dropped_from_async_task() {
        tauri::async_runtime::block_on(async {
            // 真正运行在异步工作线程中，复现原命令的调用环境。
            tauri::async_runtime::spawn(async {
                let result = run_blocking(|| {
                    let client = reqwest::blocking::Client::builder().build().unwrap();
                    drop(client);
                    Ok(())
                })
                .await;
                assert!(result.is_ok(), "{result:?}");
            })
            .await
            .unwrap();
        });
    }

    #[test]
    fn blocking_task_preserves_update_errors() {
        tauri::async_runtime::block_on(async {
            let result = run_blocking(|| Err::<(), _>(update::UpdateError::NotConfigured)).await;
            assert_eq!(
                result.unwrap_err().to_string(),
                map_err(update::UpdateError::NotConfigured).to_string()
            );
        });
    }
}
