use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::BTreeSet;

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn render(conn: &Connection, doc: &Value) -> AppResult<(String, String, BTreeSet<String>)> {
    if doc.to_string().len() > 2 * 1024 * 1024 || doc["type"] != "doc" {
        return Err(AppError::InvalidInput("图文内容格式或大小无效".into()));
    }
    let mut text = String::new();
    let mut ids = BTreeSet::new();
    let html = node(conn, doc, 0, &mut text, &mut ids)?;
    Ok((html, text.trim_end().into(), ids))
}

fn node(
    conn: &Connection,
    value: &Value,
    depth: usize,
    text: &mut String,
    ids: &mut BTreeSet<String>,
) -> AppResult<String> {
    if depth > 40 {
        return Err(AppError::InvalidInput("图文嵌套过深".into()));
    }
    let kind = value["type"].as_str().unwrap_or("");
    if kind == "text" {
        let raw = value["text"].as_str().unwrap_or("");
        text.push_str(raw);
        let mut html = escape(raw);
        if let Some(marks) = value["marks"].as_array() {
            for mark in marks {
                let tag = match mark["type"].as_str().unwrap_or("") {
                    "bold" => "strong",
                    "italic" => "em",
                    "strike" => "s",
                    "code" => "code",
                    "underline" => "u",
                    _ => return Err(AppError::InvalidInput("不支持的文字格式".into())),
                };
                html = format!("<{tag}>{html}</{tag}>");
            }
        }
        return Ok(html);
    }
    if kind == "image" {
        let id = value["attrs"]["src"]
            .as_str()
            .and_then(|s| s.strip_prefix("promptcast-image:"))
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| AppError::InvalidInput("图片必须先保存到本机".into()))?;
        let image = super::images::get(conn, id)?;
        ids.insert(id.into());
        text.push_str("[图片]\n");
        return Ok(format!("<img src=\"{}\" alt=\"图片\">", image.data_url));
    }
    if kind == "hardBreak" {
        text.push('\n');
        return Ok("<br>".into());
    }
    if kind == "horizontalRule" {
        text.push('\n');
        return Ok("<hr>".into());
    }
    let tag = match kind {
        "doc" => "",
        "paragraph" => "p",
        "bulletList" => "ul",
        "orderedList" => "ol",
        "listItem" => "li",
        "blockquote" => "blockquote",
        "codeBlock" => "pre",
        "heading" => match value["attrs"]["level"].as_u64().unwrap_or(2) {
            1 => "h1",
            2 => "h2",
            _ => "h3",
        },
        _ => return Err(AppError::InvalidInput("不支持的图文节点".into())),
    };
    let mut html = String::new();
    if let Some(children) = value["content"].as_array() {
        for child in children {
            html.push_str(&node(conn, child, depth + 1, text, ids)?);
            if html.len() > 32 * 1024 * 1024 {
                return Err(AppError::InvalidInput(
                    "单条图文内容过大，请拆分保存".into(),
                ));
            }
        }
    }
    if kind != "doc" {
        text.push('\n');
    }
    Ok(if tag.is_empty() {
        html
    } else {
        format!("<{tag}>{html}</{tag}>")
    })
}

pub fn set(conn: &Connection, id: i64, doc: &Value) -> AppResult<()> {
    let (_, text, images) = render(conn, doc)?;
    conn.execute(
        "UPDATE prompts SET rich_content=?1,content=?2 WHERE id=?3",
        params![doc.to_string(), text, id],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    conn.execute("DELETE FROM prompt_images WHERE prompt_id=?1", [id])
        .map_err(|e| AppError::Db(e.to_string()))?;
    for image in images {
        conn.execute(
            "INSERT INTO prompt_images(prompt_id,image_id) VALUES(?1,?2)",
            params![id, image],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    }
    Ok(())
}

pub fn image_doc(id: &str) -> Value {
    serde_json::json!({"type":"doc","content":[{"type":"image","attrs":{"src":format!("promptcast-image:{id}")}}]})
}
