use crate::error::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use image::{GenericImageView, ImageReader};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Cursor;

pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
const MAX_CACHE_BYTES: i64 = 512 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
pub struct ImageAsset {
    pub id: String,
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

pub fn normalize(bytes: &[u8]) -> AppResult<(Vec<u8>, u32, u32)> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(AppError::InvalidInput("图片不能超过 20 MB".into()));
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let img = reader
        .decode()
        .map_err(|e| AppError::InvalidInput(format!("无法读取图片: {e}")))?;
    let (w, h) = img.dimensions();
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let png = out.into_inner();
    if png.len() > MAX_IMAGE_BYTES {
        return Err(AppError::InvalidInput("图片解码后超过 20 MB".into()));
    }
    Ok((png, w, h))
}

pub fn store(conn: &Connection, png: &[u8], width: u32, height: u32) -> AppResult<String> {
    let id = format!("{:x}", Sha256::digest(png));
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM images WHERE id=?1)",
            [&id],
            |r| r.get(0),
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    if !exists {
        let used: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(length(png) + length(thumbnail)),0) FROM images",
                [],
                |r| r.get(0),
            )
            .map_err(|e| AppError::Db(e.to_string()))?;
        let preview = image::load_from_memory(png)
            .map_err(|e| AppError::InvalidInput(e.to_string()))?
            .thumbnail(256, 256);
        let mut thumb = Cursor::new(Vec::new());
        preview
            .write_to(&mut thumb, image::ImageFormat::Png)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        if used + png.len() as i64 + thumb.get_ref().len() as i64 > MAX_CACHE_BYTES {
            return Err(AppError::InvalidInput(
                "本地图片空间已达 512 MB，未删除已有资料".into(),
            ));
        }
        conn.execute(
            "INSERT INTO images(id,png,width,height,created_at,thumbnail) VALUES(?1,?2,?3,?4,?5,?6)",
            params![id, png, width, height, super::now_ms(), thumb.into_inner()],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    }
    Ok(id)
}

pub fn bytes(conn: &Connection, id: &str) -> AppResult<Vec<u8>> {
    conn.query_row("SELECT png FROM images WHERE id=?1", [id], |r| r.get(0))
        .map_err(|e| AppError::NotFound(format!("图片不可用: {e}")))
}

pub fn get(conn: &Connection, id: &str) -> AppResult<ImageAsset> {
    let (png, width, height): (Vec<u8>, u32, u32) = conn
        .query_row(
            "SELECT png,width,height FROM images WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(ImageAsset {
        id: id.into(),
        data_url: format!("data:image/png;base64,{}", B64.encode(png)),
        width,
        height,
    })
}

// 只删除无引用且超过一天的缓存；为正在编辑、尚未保存的图片留出时间。
pub fn collect_unused(conn: &Connection) -> AppResult<()> {
    conn.execute("DELETE FROM images WHERE created_at < ?1 AND id NOT IN (SELECT image_id FROM prompt_images) AND id NOT IN (SELECT image_id FROM clipboard_history WHERE image_id IS NOT NULL)", [super::now_ms() - 86_400_000]).map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

pub fn thumbnail(conn: &Connection, id: &str) -> AppResult<ImageAsset> {
    let (png, width, height): (Vec<u8>, u32, u32) = conn
        .query_row(
            "SELECT thumbnail,width,height FROM images WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(ImageAsset {
        id: id.into(),
        data_url: format!("data:image/png;base64,{}", B64.encode(png)),
        width,
        height,
    })
}
