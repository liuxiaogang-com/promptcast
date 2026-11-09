// db/settings.rs — settings 表是简单 key/value，前端拿强类型 Settings。
use rusqlite::{params, Connection};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::prompt::SortMode;
use crate::models::settings::{DefaultAction, Settings, ThemeMode};

const KEYS: &[&str] = &[
    "hotkey",
    "theme",
    "default_action",
    "pin_default",
    "sort_mode",
    "auto_start",
    "accessibility_granted",
    "clipboard_history_enabled",
    "clipboard_history_limit",
];

fn read_string(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    match conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |r| r.get::<_, String>(0),
    ) {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Db(e.to_string())),
    }
}

fn parse<T: for<'de> serde::Deserialize<'de>>(s: &str) -> AppResult<T> {
    serde_json::from_str(s).map_err(AppError::from)
}

fn write_value<T: serde::Serialize>(conn: &Connection, key: &str, val: &T) -> AppResult<()> {
    let s = serde_json::to_string(val)?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, s],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

/// 原始 key/value（不走 JSON 编码）。给 sync 服务器地址等内部配置用 —— 这些 key 不在 KEYS 里，
/// 不进强类型 Settings、不泄漏到前端设置页。
pub fn get_raw(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    read_string(conn, key)
}
pub fn set_raw(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

pub fn get_all(conn: &Connection) -> AppResult<Settings> {
    let mut s = Settings {
        hotkey: Some("CmdOrCtrl+Shift+P".into()),
        theme: ThemeMode::System,
        default_action: DefaultAction::Inject,
        pin_default: false,
        sort_mode: SortMode::RecentUsed,
        auto_start: false,
        accessibility_granted: false,
        clipboard_history_enabled: true,
        clipboard_history_limit: 500,
    };
    for key in KEYS {
        if let Some(raw) = read_string(conn, key)? {
            match *key {
                "hotkey" => s.hotkey = parse::<Option<String>>(&raw)?,
                "theme" => s.theme = parse(&raw)?,
                "default_action" => s.default_action = parse(&raw)?,
                "pin_default" => s.pin_default = parse(&raw)?,
                "sort_mode" => s.sort_mode = parse(&raw)?,
                "auto_start" => s.auto_start = parse(&raw)?,
                "accessibility_granted" => s.accessibility_granted = parse(&raw)?,
                "clipboard_history_enabled" => s.clipboard_history_enabled = parse(&raw)?,
                "clipboard_history_limit" => s.clipboard_history_limit = parse(&raw)?,
                _ => {}
            }
        }
    }
    Ok(s)
}

/// 通用 set：前端传 key + JSON value（Value）。
pub fn set(conn: &Connection, key: &str, value: &Value) -> AppResult<()> {
    if !KEYS.contains(&key) {
        return Err(AppError::InvalidInput(format!(
            "unknown settings key: {key}"
        )));
    }
    write_value(conn, key, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_settings_have_valid_default_hotkey() {
        let conn = crate::db::memory_conn();
        let hotkey = get_all(&conn).unwrap().hotkey.unwrap();
        assert_eq!(hotkey, "CmdOrCtrl+Shift+P");
        assert!(hotkey
            .parse::<tauri_plugin_global_shortcut::Shortcut>()
            .is_ok());
    }

    #[test]
    fn saved_hotkey_and_explicit_disable_override_default() {
        let conn = crate::db::memory_conn();
        set(&conn, "hotkey", &serde_json::json!("Alt+Shift+P")).unwrap();
        assert_eq!(
            get_all(&conn).unwrap().hotkey.as_deref(),
            Some("Alt+Shift+P")
        );
        set(&conn, "hotkey", &Value::Null).unwrap();
        assert_eq!(get_all(&conn).unwrap().hotkey, None);
    }
}
