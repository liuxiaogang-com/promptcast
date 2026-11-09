// db/schema.rs — 当前 schema 版本的建表 SQL（migrations 引用）。
pub const V1: &str = r#"
CREATE TABLE IF NOT EXISTS folders (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT NOT NULL UNIQUE,
  sort_order  INTEGER NOT NULL DEFAULT 0,
  created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tags (
  id     INTEGER PRIMARY KEY AUTOINCREMENT,
  name   TEXT NOT NULL UNIQUE,
  color  TEXT
);

CREATE TABLE IF NOT EXISTS prompts (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  title        TEXT NOT NULL,
  content      TEXT NOT NULL,
  folder_id    INTEGER REFERENCES folders(id) ON DELETE SET NULL,
  is_favorite  INTEGER NOT NULL DEFAULT 0,
  is_pinned    INTEGER NOT NULL DEFAULT 0,
  use_count    INTEGER NOT NULL DEFAULT 0,
  last_used_at INTEGER,
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS prompt_tags (
  prompt_id INTEGER NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
  tag_id    INTEGER NOT NULL REFERENCES tags(id)    ON DELETE CASCADE,
  PRIMARY KEY (prompt_id, tag_id)
);

CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sites (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  name               TEXT NOT NULL,
  url                TEXT NOT NULL,
  favicon_blob       BLOB,
  favicon_mime       TEXT,
  favicon_fetched_at INTEGER,
  sort_order         INTEGER NOT NULL DEFAULT 0,
  created_at         INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_prompts_folder    ON prompts(folder_id);
CREATE INDEX IF NOT EXISTS idx_prompts_lastused  ON prompts(last_used_at);
CREATE INDEX IF NOT EXISTS idx_prompts_pinned    ON prompts(is_pinned, is_favorite);
"#;

// V2 — 剪贴板历史（仅文本）。后台监听 changeCount，自动入库。
pub const V2: &str = r#"
CREATE TABLE IF NOT EXISTS clipboard_history (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  content     TEXT NOT NULL,
  char_count  INTEGER NOT NULL,
  created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_clip_created ON clipboard_history(created_at DESC);
"#;

// V3 — 同步就绪：给可同步实体（folders/tags/prompts/sites）加全局 uuid、软删墓碑
// deleted_at、待推送标志 dirty，并补齐缺失的 updated_at / created_at。纯加列（安全）。
// 加 DEFAULT 的列会自动给既有行填默认值（dirty=1 ⇒ 迁移后整库排队首次 push）。
// uuid 的回填在 migrations.rs 里用 Rust 生成（execute_batch 不能生成 uuid）。
// 详见 plan：离线优先多设备同步 Phase 0。
pub const V3: &str = r#"
ALTER TABLE folders ADD COLUMN uuid       TEXT;
ALTER TABLE folders ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE folders ADD COLUMN deleted_at INTEGER;
ALTER TABLE folders ADD COLUMN dirty      INTEGER NOT NULL DEFAULT 1;

ALTER TABLE tags ADD COLUMN uuid       TEXT;
ALTER TABLE tags ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tags ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tags ADD COLUMN deleted_at INTEGER;
ALTER TABLE tags ADD COLUMN dirty      INTEGER NOT NULL DEFAULT 1;

ALTER TABLE prompts ADD COLUMN uuid       TEXT;
ALTER TABLE prompts ADD COLUMN deleted_at INTEGER;
ALTER TABLE prompts ADD COLUMN dirty      INTEGER NOT NULL DEFAULT 1;

ALTER TABLE sites ADD COLUMN uuid       TEXT;
ALTER TABLE sites ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sites ADD COLUMN deleted_at INTEGER;
ALTER TABLE sites ADD COLUMN dirty      INTEGER NOT NULL DEFAULT 1;

CREATE UNIQUE INDEX IF NOT EXISTS idx_folders_uuid ON folders(uuid) WHERE uuid IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_tags_uuid    ON tags(uuid)    WHERE uuid IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_prompts_uuid ON prompts(uuid) WHERE uuid IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_sites_uuid   ON sites(uuid)   WHERE uuid IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_folders_dirty ON folders(dirty) WHERE dirty = 1;
CREATE INDEX IF NOT EXISTS idx_tags_dirty    ON tags(dirty)    WHERE dirty = 1;
CREATE INDEX IF NOT EXISTS idx_prompts_dirty ON prompts(dirty) WHERE dirty = 1;
CREATE INDEX IF NOT EXISTS idx_sites_dirty   ON sites(dirty)   WHERE dirty = 1;

CREATE TABLE IF NOT EXISTS sync_state (
  id               INTEGER PRIMARY KEY CHECK (id = 1),
  device_id        TEXT NOT NULL,
  last_pull_cursor INTEGER NOT NULL DEFAULT 0,
  last_sync_at     INTEGER,
  user_id          TEXT,
  sync_enabled     INTEGER NOT NULL DEFAULT 0
);
"#;

// V4 — 把 folders/tags 的 name 列级 UNIQUE 改成「仅存活行唯一」的部分唯一索引。
// 原因（review 确认）：软删墓碑仍占着 name，导致 30 天内无法重建同名文件夹/标签；且
// 跨设备同名收编 / 重命名互换在 apply 时会撞 UNIQUE。SQLite 不能直接删列约束 → 重建表。
// 重建期间必须 foreign_keys=OFF（见 migrations.rs），且整体在事务里保证原子。
pub const V4: &str = r#"
BEGIN;
CREATE TABLE folders_new (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT NOT NULL,
  sort_order  INTEGER NOT NULL DEFAULT 0,
  created_at  INTEGER NOT NULL,
  uuid        TEXT,
  updated_at  INTEGER NOT NULL DEFAULT 0,
  deleted_at  INTEGER,
  dirty       INTEGER NOT NULL DEFAULT 1
);
INSERT INTO folders_new (id, name, sort_order, created_at, uuid, updated_at, deleted_at, dirty)
  SELECT id, name, sort_order, created_at, uuid, updated_at, deleted_at, dirty FROM folders;
DROP TABLE folders;
ALTER TABLE folders_new RENAME TO folders;

CREATE TABLE tags_new (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT NOT NULL,
  color       TEXT,
  uuid        TEXT,
  created_at  INTEGER NOT NULL DEFAULT 0,
  updated_at  INTEGER NOT NULL DEFAULT 0,
  deleted_at  INTEGER,
  dirty       INTEGER NOT NULL DEFAULT 1
);
INSERT INTO tags_new (id, name, color, uuid, created_at, updated_at, deleted_at, dirty)
  SELECT id, name, color, uuid, created_at, updated_at, deleted_at, dirty FROM tags;
DROP TABLE tags;
ALTER TABLE tags_new RENAME TO tags;

CREATE UNIQUE INDEX idx_folders_uuid      ON folders(uuid) WHERE uuid IS NOT NULL;
CREATE INDEX        idx_folders_dirty     ON folders(dirty) WHERE dirty = 1;
CREATE UNIQUE INDEX idx_folders_name_live ON folders(name) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX idx_tags_uuid         ON tags(uuid) WHERE uuid IS NOT NULL;
CREATE INDEX        idx_tags_dirty        ON tags(dirty) WHERE dirty = 1;
CREATE UNIQUE INDEX idx_tags_name_live    ON tags(name) WHERE deleted_at IS NULL;
COMMIT;
"#;

// V5 — 图文正文和本地图片。旧文本保持原样，所有新增列均可为空。
pub const V5: &str = r#"
BEGIN IMMEDIATE;
ALTER TABLE prompts ADD COLUMN rich_content TEXT;
ALTER TABLE clipboard_history ADD COLUMN image_id TEXT REFERENCES images(id);
ALTER TABLE clipboard_history ADD COLUMN saved_prompt_id INTEGER REFERENCES prompts(id) ON DELETE SET NULL;
CREATE TABLE images (
  id TEXT PRIMARY KEY,
  png BLOB NOT NULL,
  thumbnail BLOB NOT NULL,
  width INTEGER NOT NULL,
  height INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE prompt_images (
  prompt_id INTEGER NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
  image_id TEXT NOT NULL REFERENCES images(id),
  PRIMARY KEY (prompt_id, image_id)
);
PRAGMA user_version = 5;
COMMIT;
"#;
