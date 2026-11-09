// models/prompt.rs — 镜像 src/types/prompt.ts。
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Prompt {
    pub id: i64,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub rich_content: Option<serde_json::Value>,
    pub folder_id: Option<i64>,
    pub tag_ids: Vec<i64>,
    pub is_favorite: bool,
    pub is_pinned: bool,
    pub use_count: i64,
    pub last_used_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PromptDraft {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub rich_content: Option<serde_json::Value>,
    pub folder_id: Option<i64>,
    pub tag_ids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortMode {
    RecentUsed,
    Created,
    Updated,
    Title,
}
