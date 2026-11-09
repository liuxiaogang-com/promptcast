// sync/client.rs — 同步后端的 blocking HTTP 客户端（复用 sites.rs 的 reqwest 套路）。
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::models::sync::Change;

#[derive(Debug)]
pub enum SyncError {
    Network(String),
    Unauthorized,
    Status(u16, String),
    Parse(String),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Network(s) => write!(f, "network: {s}"),
            SyncError::Unauthorized => write!(f, "unauthorized"),
            SyncError::Status(c, b) => write!(f, "http {c}: {b}"),
            SyncError::Parse(s) => write!(f, "parse: {s}"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthResponse {
    #[serde(rename = "userId")]
    pub user_id: String,
    pub access: String,
    pub refresh: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokensResponse {
    pub access: String,
    pub refresh: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullResponse {
    pub changes: Vec<Change>,
    pub next_cursor: i64,
    pub has_more: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PushItemResult {
    pub uuid: String,
    pub applied: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PushResponse {
    pub results: Vec<PushItemResult>,
}

#[derive(Serialize)]
struct Credentials<'a> {
    email: &'a str,
    password: &'a str,
}

#[derive(Serialize)]
struct RefreshBody<'a> {
    refresh: &'a str,
}

#[derive(Serialize)]
struct PullBody {
    since_cursor: i64,
    limit: i64,
}

#[derive(Serialize)]
struct PushBody<'a> {
    changes: &'a [Change],
}

pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
}

impl Client {
    pub fn new(base: &str) -> Self {
        let http = reqwest::blocking::Client::builder()
            .user_agent("PromptCast/0.1")
            .timeout(Duration::from_secs(20))
            .build()
            .expect("reqwest client");
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn parse<T: for<'de> Deserialize<'de>>(
        resp: reqwest::blocking::Response,
    ) -> Result<T, SyncError> {
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::Unauthorized);
        }
        if !status.is_success() {
            return Err(SyncError::Status(
                status.as_u16(),
                resp.text().unwrap_or_default(),
            ));
        }
        resp.json::<T>()
            .map_err(|e| SyncError::Parse(e.to_string()))
    }

    pub fn register(&self, email: &str, password: &str) -> Result<AuthResponse, SyncError> {
        let resp = self
            .http
            .post(self.url("/auth/register"))
            .json(&Credentials { email, password })
            .send()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        Self::parse(resp)
    }

    pub fn login(&self, email: &str, password: &str) -> Result<AuthResponse, SyncError> {
        let resp = self
            .http
            .post(self.url("/auth/login"))
            .json(&Credentials { email, password })
            .send()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        Self::parse(resp)
    }

    pub fn refresh(&self, refresh_token: &str) -> Result<TokensResponse, SyncError> {
        let resp = self
            .http
            .post(self.url("/auth/refresh"))
            .json(&RefreshBody {
                refresh: refresh_token,
            })
            .send()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        Self::parse(resp)
    }

    pub fn logout(&self, refresh_token: &str) -> Result<(), SyncError> {
        let resp = self
            .http
            .post(self.url("/auth/logout"))
            .json(&RefreshBody {
                refresh: refresh_token,
            })
            .send()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(SyncError::Status(
                status.as_u16(),
                resp.text().unwrap_or_default(),
            ))
        }
    }

    pub fn pull(
        &self,
        access: &str,
        since_cursor: i64,
        limit: i64,
    ) -> Result<PullResponse, SyncError> {
        let resp = self
            .http
            .post(self.url("/sync/pull"))
            .bearer_auth(access)
            .json(&PullBody {
                since_cursor,
                limit,
            })
            .send()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        Self::parse(resp)
    }

    pub fn push(&self, access: &str, changes: &[Change]) -> Result<PushResponse, SyncError> {
        let resp = self
            .http
            .post(self.url("/sync/push"))
            .bearer_auth(access)
            .json(&PushBody { changes })
            .send()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        Self::parse(resp)
    }

    /// 期待 2xx、无响应体（204 等）。
    fn expect_success(resp: reqwest::blocking::Response) -> Result<(), SyncError> {
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::Unauthorized);
        }
        if status.is_success() {
            Ok(())
        } else {
            Err(SyncError::Status(
                status.as_u16(),
                resp.text().unwrap_or_default(),
            ))
        }
    }

    pub fn change_password(
        &self,
        access: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), SyncError> {
        let resp = self
            .http
            .post(self.url("/auth/change-password"))
            .bearer_auth(access)
            .json(&serde_json::json!({
                "oldPassword": old_password,
                "newPassword": new_password,
            }))
            .send()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        Self::expect_success(resp)
    }

    pub fn delete_account(&self, access: &str, password: &str) -> Result<(), SyncError> {
        let resp = self
            .http
            .post(self.url("/auth/delete-account"))
            .bearer_auth(access)
            .json(&serde_json::json!({ "password": password }))
            .send()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        Self::expect_success(resp)
    }

    pub fn forgot_password(&self, email: &str) -> Result<ForgotResponse, SyncError> {
        let resp = self
            .http
            .post(self.url("/auth/forgot-password"))
            .json(&serde_json::json!({ "email": email }))
            .send()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        Self::parse(resp)
    }

    pub fn reset_password(&self, token: &str, new_password: &str) -> Result<(), SyncError> {
        let resp = self
            .http
            .post(self.url("/auth/reset-password"))
            .json(&serde_json::json!({ "token": token, "newPassword": new_password }))
            .send()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        Self::expect_success(resp)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgotResponse {
    /// 仅本地开发回显；生产为空（应改走邮件）。
    #[serde(rename = "devToken", default)]
    pub dev_token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prompt_change(uuid: &str, updated_at: i64, title: &str) -> Change {
        Change {
            entity: "prompt".into(),
            uuid: uuid.into(),
            updated_at,
            deleted_at: None,
            data: json!({
                "title": title, "content": "x", "is_favorite": false, "is_pinned": false,
                "created_at": 1, "folder_uuid": null, "tag_uuids": []
            }),
            seq: 0,
        }
    }

    // 实机集成测试：验证 Rust 客户端 ↔ 真实服务端的序列化/LWW 全链路。
    // 需要本地服务端在 http://localhost:3000（或设 SYNC_TEST_URL）。默认 #[ignore]，手动跑：
    //   cargo test --manifest-path src-tauri/Cargo.toml --lib sync::client::tests -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_register_push_pull_lww() {
        let base =
            std::env::var("SYNC_TEST_URL").unwrap_or_else(|_| "http://localhost:3000".into());
        let c = Client::new(&base);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let email = format!("rust{nanos}@test.io");
        let auth = c.register(&email, "password123").expect("register");
        assert!(!auth.access.is_empty(), "got access token");

        let uuid = "22222222-2222-4222-8222-222222222222";
        let push = c
            .push(&auth.access, &[prompt_change(uuid, 1000, "v1")])
            .expect("push");
        assert!(push.results[0].applied, "first push applied");

        let pull = c.pull(&auth.access, 0, 100).expect("pull");
        assert_eq!(pull.changes.len(), 1, "pull returns the pushed change");
        assert_eq!(pull.changes[0].uuid, uuid);

        // 更新覆盖。
        let p2 = c
            .push(&auth.access, &[prompt_change(uuid, 2000, "v2")])
            .expect("push v2");
        assert!(p2.results[0].applied);

        // LWW：过期 push 被拒。
        let stale = c
            .push(&auth.access, &[prompt_change(uuid, 1500, "old")])
            .expect("push stale");
        assert!(!stale.results[0].applied, "stale push rejected by LWW");

        // 从 cursor 0 再拉，应拿到 v2。
        let pull2 = c.pull(&auth.access, 0, 100).expect("pull2");
        let rec = pull2.changes.iter().find(|c| c.uuid == uuid).unwrap();
        assert_eq!(rec.data["title"], "v2");
    }

    // 双设备端到端：两个独立本地库 + 真实服务端，验证 创建/删除 的收敛。
    //   cargo test --manifest-path src-tauri/Cargo.toml --lib sync::client::tests::live_two_device -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_two_device_convergence() {
        use crate::db::{folders, memory_conn, prompts, sync_repo};
        use crate::models::prompt::{PromptDraft, SortMode};

        let base =
            std::env::var("SYNC_TEST_URL").unwrap_or_else(|_| "http://localhost:3000".into());
        let c = Client::new(&base);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let auth = c
            .register(&format!("two{nanos}@test.io"), "password123")
            .expect("register");
        let token = auth.access;

        let order = |e: &str| match e {
            "folder" => 0u8,
            "tag" => 1,
            "prompt" => 2,
            _ => 3,
        };
        let push_dirty = |db: &mut rusqlite::Connection| {
            let dirty = sync_repo::collect_dirty(db).unwrap();
            if dirty.is_empty() {
                return;
            }
            let resp = c.push(&token, &dirty).expect("push");
            for item in &resp.results {
                if item.applied {
                    if let Some(ch) = dirty.iter().find(|x| x.uuid == item.uuid) {
                        sync_repo::clear_dirty(db, &ch.entity, &item.uuid, ch.updated_at).unwrap();
                    }
                }
            }
        };
        let pull_apply = |db: &mut rusqlite::Connection, cursor: i64| -> i64 {
            let pull = c.pull(&token, cursor, 500).expect("pull");
            let mut batch = pull.changes.clone();
            batch.sort_by_key(|x| order(&x.entity));
            for ch in &batch {
                sync_repo::apply_change(db, ch).unwrap();
            }
            pull.next_cursor
        };

        // 设备 A：建 folder + prompt，推上去。
        let mut a = memory_conn();
        let f = folders::create(&a, "Work").unwrap().id;
        let p = prompts::create(
            &mut a,
            PromptDraft {
                rich_content: None,
                title: "hello".into(),
                content: "body".into(),
                folder_id: Some(f),
                tag_ids: vec![],
            },
        )
        .unwrap();
        push_dirty(&mut a);

        // 设备 B：空库，拉 + apply → 应收到 A 的 prompt（且 folder 引用解析成功）。
        let mut b = memory_conn();
        let cursor_b = pull_apply(&mut b, 0);
        let list_b = prompts::list(&b, SortMode::Created).unwrap();
        assert_eq!(list_b.len(), 1, "B received A's prompt");
        assert_eq!(list_b[0].title, "hello");
        assert!(list_b[0].folder_id.is_some(), "folder_uuid resolved on B");

        // A 删除该 prompt → 推墓碑；B 再拉 → prompt 隐藏（学到删除）。
        prompts::delete(&mut a, p.id).unwrap();
        push_dirty(&mut a);
        pull_apply(&mut b, cursor_b);
        assert!(
            prompts::list(&b, SortMode::Created).unwrap().is_empty(),
            "B converges on the deletion"
        );
    }
}
