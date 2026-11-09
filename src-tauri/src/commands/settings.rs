// commands/settings.rs
use serde_json::Value;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::db::{self, DbState};
use crate::error::{AppError, AppResult};
use crate::events;
use crate::models::settings::Settings;

#[tauri::command]
pub fn settings_get_all(db: State<'_, DbState>) -> AppResult<Settings> {
    let conn = db.0.lock();
    db::settings::get_all(&conn)
}

#[tauri::command]
pub fn settings_set(
    app: AppHandle,
    db: State<'_, DbState>,
    key: String,
    value: Value,
) -> AppResult<()> {
    if key == "hotkey" {
        let hotkey: Option<String> = serde_json::from_value(value)?;
        return set_hotkey(&app, hotkey);
    }
    {
        let conn = db.0.lock();
        db::settings::set(&conn, &key, &value)?;
    }
    tracing::info!(%key, value = %value, "settings updated");
    events::emit_settings_changed(&app, &key);
    if key == "theme" {
        if let Some(s) = value.as_str() {
            events::emit_theme_changed(&app, s);
        }
    }
    if key == "auto_start" {
        if let Some(enabled) = value.as_bool() {
            apply_autostart(&app, enabled);
        }
    }
    Ok(())
}

/// 把 auto_start 状态同步到系统（macOS LaunchAgent / Windows 注册表）。
/// 仅在状态确实需要变更时才调用，避免 disable 一个不存在的项导致 ENOENT。
pub fn apply_autostart(app: &AppHandle, enabled: bool) {
    let mgr = app.autolaunch();
    let current = match mgr.is_enabled() {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "autostart is_enabled failed; skipping sync");
            return;
        }
    };
    if current == enabled {
        return;
    }
    let res = if enabled { mgr.enable() } else { mgr.disable() };
    match res {
        Ok(()) => tracing::info!(enabled, "autostart applied"),
        Err(e) => tracing::error!(enabled, error = %e, "autostart apply failed"),
    }
}

/// 先取得新快捷键，再提交设置，最后释放旧键。任何保存失败都保留旧键。
pub fn set_hotkey(app: &AppHandle, hotkey: Option<String>) -> AppResult<()> {
    use tauri::Manager;
    let new = hotkey
        .as_deref()
        .map(str::parse::<Shortcut>)
        .transpose()
        .map_err(|e| AppError::InvalidInput(format!("无效快捷键：{e}")))?;
    let db = app.state::<DbState>();
    let mut conn = db.0.lock();
    let old = db::settings::get_all(&conn)?
        .hotkey
        .as_deref()
        .and_then(|s| s.parse::<Shortcut>().ok())
        .filter(|s| app.global_shortcut().is_registered(*s));
    let added = new.filter(|s| Some(*s) != old);
    persist_hotkey(
        &mut conn,
        &hotkey,
        || {
            if added.is_some() {
                crate::register_global_hotkey(app, hotkey.as_deref().unwrap())
                    .map_err(AppError::Internal)?;
            }
            Ok(())
        },
        || {
            if let Some(shortcut) = added {
                if let Err(e) = app.global_shortcut().unregister(shortcut) {
                    tracing::error!(error = %e, "failed to release unsaved hotkey");
                }
            }
        },
    )?;
    drop(conn);
    events::emit_settings_changed(app, "hotkey");
    if let Some(shortcut) = old.filter(|s| Some(*s) != new) {
        app.global_shortcut().unregister(shortcut).map_err(|e| {
            AppError::Internal(format!("新设置已保存，但旧快捷键释放失败，请重启应用：{e}"))
        })?;
    }
    Ok(())
}

fn persist_hotkey(
    conn: &mut rusqlite::Connection,
    hotkey: &Option<String>,
    register: impl FnOnce() -> AppResult<()>,
    undo_registration: impl FnOnce(),
) -> AppResult<()> {
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    db::settings::set(&tx, "hotkey", &serde_json::to_value(hotkey)?)?;
    register()?;
    if let Err(e) = tx.commit() {
        undo_registration();
        return Err(AppError::Db(e.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn hotkey_conflict_preserves_saved_key() {
        let mut conn = db::memory_conn();
        let old = db::settings::get_all(&conn).unwrap().hotkey;
        let result = persist_hotkey(
            &mut conn,
            &Some("Alt+P".into()),
            || Err(AppError::Internal("occupied".into())),
            || panic!("nothing registered"),
        );
        assert!(result.is_err());
        assert_eq!(db::settings::get_all(&conn).unwrap().hotkey, old);
    }

    #[test]
    fn failed_commit_releases_new_registration_and_restores_old_setting() {
        let mut conn = db::memory_conn();
        db::settings::set(&conn, "hotkey", &serde_json::json!("Alt+P")).unwrap();
        conn.execute_batch("CREATE TABLE parent(id INTEGER PRIMARY KEY);
            CREATE TABLE deferred_child(id INTEGER REFERENCES parent(id) DEFERRABLE INITIALLY DEFERRED);
            CREATE TRIGGER fail_commit AFTER UPDATE ON settings WHEN NEW.key='hotkey'
            BEGIN INSERT INTO deferred_child VALUES(1); END;").unwrap();
        let registered = Cell::new(false);
        let result = persist_hotkey(
            &mut conn,
            &Some("Alt+Q".into()),
            || {
                registered.set(true);
                Ok(())
            },
            || registered.set(false),
        );
        assert!(result.is_err());
        assert!(!registered.get());
        assert_eq!(
            db::settings::get_all(&conn).unwrap().hotkey.as_deref(),
            Some("Alt+P")
        );
    }
}
