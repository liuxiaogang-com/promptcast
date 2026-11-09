// models/settings.rs — 强类型 Settings；DB 里以 key/value 存储。
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DefaultAction {
    Inject,
    CopyOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Settings {
    pub hotkey: Option<String>,
    pub theme: ThemeMode,
    pub font_scale: f64,
    pub default_action: DefaultAction,
    pub pin_default: bool,
    pub sort_mode: super::prompt::SortMode,
    pub auto_start: bool,
    pub accessibility_granted: bool,
    pub clipboard_history_enabled: bool,
    pub clipboard_history_limit: u32,
}
