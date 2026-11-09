use crate::models::prompt::Prompt;
use crate::{
    db::{self, DbState},
    error::{AppError, AppResult},
    events,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State};

#[tauri::command]
pub fn image_import(db: State<'_, DbState>, data: String) -> AppResult<String> {
    if data.len() > db::images::MAX_IMAGE_BYTES * 4 / 3 + 4 {
        return Err(AppError::InvalidInput("图片不能超过 20 MB".into()));
    }
    let bytes = B64
        .decode(data)
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    let (png, w, h) = db::images::normalize(&bytes)?;
    let conn = db.0.lock();
    db::images::collect_unused(&conn)?;
    db::images::store(&conn, &png, w, h)
}

#[tauri::command]
pub fn image_get(
    db: State<'_, DbState>,
    id: String,
    thumbnail: bool,
) -> AppResult<db::images::ImageAsset> {
    if thumbnail {
        db::images::thumbnail(&db.0.lock(), &id)
    } else {
        db::images::get(&db.0.lock(), &id)
    }
}

pub fn save_clip(conn: &mut rusqlite::Connection, id: i64, favorite: bool) -> AppResult<Prompt> {
    let err = |e: rusqlite::Error| AppError::Db(e.to_string());
    let tx = conn.transaction().map_err(err)?;
    let (text, image, saved): (String, Option<String>, Option<i64>) = tx
        .query_row(
            "SELECT content,image_id,saved_prompt_id FROM clipboard_history WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(err)?;
    let existing = match saved {
        Some(pid) => tx
            .query_row(
                "SELECT id FROM prompts WHERE id=?1 AND deleted_at IS NULL",
                [pid],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(err)?,
        None => None,
    };
    let pid = if let Some(pid) = existing {
        pid
    } else {
        let title = if image.is_some() {
            "剪贴板图片".into()
        } else {
            text.lines()
                .next()
                .unwrap_or("剪贴板内容")
                .chars()
                .take(40)
                .collect::<String>()
        };
        tx.execute(
            "INSERT INTO prompts(uuid,title,content,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)",
            params![uuid::Uuid::new_v4().to_string(), title, text, db::now_ms()],
        )
        .map_err(err)?;
        let pid = tx.last_insert_rowid();
        if let Some(image) = image {
            db::rich::set(&tx, pid, &db::rich::image_doc(&image))?;
        }
        tx.execute(
            "UPDATE clipboard_history SET saved_prompt_id=?1 WHERE id=?2",
            params![pid, id],
        )
        .map_err(err)?;
        pid
    };
    if favorite {
        tx.execute(
            "UPDATE prompts SET is_favorite=1,dirty=1,updated_at=?1 WHERE id=?2",
            params![db::now_ms(), pid],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)?;
    db::prompts::get(conn, pid)
}

#[tauri::command]
pub fn clipboard_save(
    app: AppHandle,
    db: State<'_, DbState>,
    id: i64,
    favorite: bool,
) -> AppResult<Prompt> {
    let prompt = save_clip(&mut db.0.lock(), id, favorite)?;
    events::emit_prompts_changed(&app);
    events::emit_clipboard_changed(&app);
    Ok(prompt)
}

#[tauri::command]
pub fn content_copy(
    app: AppHandle,
    db: State<'_, DbState>,
    id: i64,
    source: String,
    paste: bool,
) -> AppResult<super::inject::InjectResult> {
    let (text, html, image) = {
        let conn = db.0.lock();
        match source.as_str() {
            "prompt" => {
                let p = db::prompts::get(&conn, id)?;
                if let Some(doc) = p.rich_content {
                    let (html, text, ids) = db::rich::render(&conn, &doc)?;
                    let only_image = doc["content"]
                        .as_array()
                        .is_some_and(|a| a.len() == 1 && a[0]["type"] == "image");
                    let image = if only_image {
                        ids.first()
                            .map(|id| db::images::bytes(&conn, id))
                            .transpose()?
                    } else {
                        None
                    };
                    (text, Some(html), image)
                } else {
                    (p.content, None, None)
                }
            }
            "clipboard" => {
                let (text, image): (String, Option<String>) = conn
                    .query_row(
                        "SELECT content,image_id FROM clipboard_history WHERE id=?1",
                        [id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .map_err(|e| AppError::Db(e.to_string()))?;
                (
                    text,
                    None,
                    image.map(|id| db::images::bytes(&conn, &id)).transpose()?,
                )
            }
            _ => return Err(AppError::InvalidInput("未知内容来源".into())),
        }
    };
    let mut cb = arboard::Clipboard::new().map_err(|e| AppError::Internal(e.to_string()))?;
    if let Some(bytes) = image {
        super::inject::note_self_copy(&format!("image:{:x}", Sha256::digest(&bytes)));
        let img = image::load_from_memory(&bytes)
            .map_err(|e| AppError::Internal(e.to_string()))?
            .to_rgba8();
        cb.set_image(arboard::ImageData {
            width: img.width() as usize,
            height: img.height() as usize,
            bytes: std::borrow::Cow::Owned(img.into_raw()),
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;
    } else if let Some(html) = html {
        cb.set_html(html, Some(text.clone()))
            .map_err(|e| AppError::Internal(e.to_string()))?;
    } else {
        if paste {
            return super::inject::inject_paste(app, text);
        }
        super::inject::inject_copy_only(text)?;
        return Ok(super::inject::InjectResult {
            ok: true,
            fallback: None,
            message: None,
        });
    }
    // 图文不使用纯文本还原逻辑，避免把图片替换成旧文本。
    if !text.is_empty() && text != "[图片]" {
        super::inject::note_self_copy(&text);
    }
    if paste {
        super::inject::paste_prepared_clipboard(app)
    } else {
        Ok(super::inject::InjectResult {
            ok: true,
            fallback: None,
            message: None,
        })
    }
}
