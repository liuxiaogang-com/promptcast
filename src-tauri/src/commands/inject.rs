// commands/inject.rs — 剪贴板写入 + 模拟粘贴。
//
// 流程：
//   1) 保存当前剪贴板（用于稍后还原）
//   2) 写新内容
//   3) AXIsProcessTrusted() 检查（macOS）；未授权直接 fallback
//   4) hide drawer（释放 key window）
//   5) 激活之前保存的目标窗口（Windows 优先 HWND，macOS 用 PID）
//   6) sleep ~120ms 让系统切焦稳定
//   7) enigo 模拟 Cmd/Ctrl+V
//   8) 异步线程 ~600ms 后还原原剪贴板
use std::{thread, time::Duration};

use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use parking_lot::Mutex;
use std::sync::OnceLock;

use crate::error::{AppError, AppResult};
use crate::platform::{self, permissions};
use crate::LastFrontmost;

// 粘贴键发出后等多久再还原原剪贴板：要给目标应用足够时间读走我们写入的内容再还原，
// 否则慢应用会粘到旧内容。Windows 上（Electron/浏览器/远程桌面较慢）给更长余量；
// 两端都有 still_ours 守卫，延长只是「原剪贴板晚一点还原」，无副作用。
#[cfg(target_os = "windows")]
const CLIPBOARD_RESTORE_DELAY_MS: u64 = 900;
#[cfg(not(target_os = "windows"))]
const CLIPBOARD_RESTORE_DELAY_MS: u64 = 600;

// 记录「我们自己刚写进剪贴板的内容」，让剪贴板历史监听线程跳过自注入/复制/还原，
// 避免把注入的提示词也记成一次外部「复制」。
static LAST_SELF_COPY: OnceLock<Mutex<Option<String>>> = OnceLock::new();
fn self_copy_cell() -> &'static Mutex<Option<String>> {
    LAST_SELF_COPY.get_or_init(|| Mutex::new(None))
}
pub fn note_self_copy(text: &str) {
    *self_copy_cell().lock() = Some(text.to_string());
}
/// 若 `text` 正是我们刚写入的内容，返回 true 并清除标记（一次性消费）。
pub fn is_recent_self_copy(text: &str) -> bool {
    let mut g = self_copy_cell().lock();
    if g.as_deref() == Some(text) {
        *g = None;
        true
    } else {
        false
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct InjectResult {
    pub ok: bool,
    pub fallback: Option<String>,
    pub message: Option<String>,
}

fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
}

fn write_clipboard(text: &str) -> AppResult<()> {
    let mut cb =
        arboard::Clipboard::new().map_err(|e| AppError::Internal(format!("clipboard: {e}")))?;
    cb.set_text(text)
        .map_err(|e| AppError::Internal(format!("clipboard set: {e}")))?;
    Ok(())
}

/// 在后台线程延时还原剪贴板。失败仅打日志，不影响主流程。
fn schedule_clipboard_restore(prev: Option<String>, just_pasted: String, delay_ms: u64) {
    let Some(prev) = prev else { return };
    if prev == just_pasted {
        return;
    }
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(delay_ms));
        if let Ok(mut cb) = arboard::Clipboard::new() {
            // 仅在剪贴板内容仍是我们注入的那段时还原 —— 避免覆盖用户期间手动复制的内容。
            let still_ours = cb.get_text().ok().as_deref() == Some(just_pasted.as_str());
            if still_ours {
                note_self_copy(&prev);
                if let Err(e) = cb.set_text(prev) {
                    tracing::warn!(error = %e, "clipboard restore failed");
                } else {
                    tracing::info!("clipboard restored");
                }
            } else {
                tracing::info!("clipboard changed by user; skipping restore");
            }
        }
    });
}

/// 轮询等待 `target_pid` 成为前台进程，最长等 `deadline`。
/// 已是前台或超时即返回，之后再给渲染进程一点缓冲。比固定 sleep 稳得多。
fn wait_until_foreground(target_pid: Option<i32>, deadline: Duration) {
    let step = Duration::from_millis(15);
    let mut waited = Duration::ZERO;
    loop {
        if let Some(tp) = target_pid {
            if platform::frontmost_app_pid() == Some(tp) {
                tracing::info!(tp, ?waited, "target is frontmost");
                break;
            }
        }
        if waited >= deadline {
            tracing::warn!(
                ?deadline,
                "target not frontmost before deadline; pasting anyway"
            );
            break;
        }
        thread::sleep(step);
        waited += step;
    }
    // 焦点到位后给渲染进程（浏览器/Electron）一点缓冲再粘贴。
    thread::sleep(Duration::from_millis(40));
}

#[tauri::command]
pub fn inject_copy_only(content: String) -> AppResult<()> {
    let len = content.len();
    write_clipboard(&content)?;
    note_self_copy(&content);
    tracing::info!(len, "clipboard written (copy_only)");
    Ok(())
}

#[tauri::command]
pub fn inject_paste(app: AppHandle, content: String) -> AppResult<InjectResult> {
    let len = content.len();
    tracing::info!(len, "inject_paste begin");

    // 1) 保存当前剪贴板，便于注入后还原
    let prev_clipboard = read_clipboard();

    // 2) 写新内容
    if let Err(e) = write_clipboard(&content) {
        tracing::error!(error = %e, "clipboard write failed");
        return Ok(InjectResult {
            ok: false,
            fallback: None,
            message: Some(format!("clipboard write failed: {e}")),
        });
    }
    note_self_copy(&content);

    // 3) 权限（macOS：AXIsProcessTrusted；Windows：恒 true）
    let trusted = permissions::is_trusted();
    tracing::info!(trusted, "permissions check");
    if !trusted {
        if let Some(w) = app.get_webview_window("drawer") {
            let _ = w.hide();
        }
        // 主动弹出系统辅助功能授权引导框（首次未授权时出现「打开系统设置」）。
        #[cfg(target_os = "macos")]
        {
            let _ = permissions::prompt_trust();
        }
        let msg = if cfg!(target_os = "macos") {
            "已弹出辅助功能授权框 · 授权后重开应用即可注入 · 本次已复制到剪贴板"
        } else {
            "键盘模拟权限不足 · 已复制到剪贴板"
        };
        return Ok(InjectResult {
            ok: false,
            fallback: Some("clipboard".to_string()),
            message: Some(msg.into()),
        });
    }

    // 4) 取出之前保存的目标窗口。若没有目标（如未经热键唤起就注入），
    //    不要盲粘 —— 会粘到桌面/本进程。直接回退「已复制」并提示。
    let target = app
        .try_state::<LastFrontmost>()
        .map(|s| *s.0.lock())
        .unwrap_or_default();
    if target.pid.is_none() && target.hwnd.is_none() {
        tracing::warn!("no saved target window; falling back to copy");
        if let Some(w) = app.get_webview_window("drawer") {
            let _ = w.hide();
        }
        return Ok(InjectResult {
            ok: false,
            fallback: Some("clipboard".to_string()),
            message: Some("未捕获目标窗口 · 已复制到剪贴板".into()),
        });
    }

    // 5) 隐藏抽屉（释放 key window），激活目标窗口。
    if let Some(w) = app.get_webview_window("drawer") {
        let _ = w.hide();
    }
    match (target.hwnd, target.pid) {
        (Some(hwnd), pid) => {
            tracing::info!(hwnd, ?pid, "activating saved target window");
            platform::activate_window_by_handle(hwnd, pid);
        }
        (None, Some(pid)) => {
            tracing::info!(pid, "activating saved frontmost pid");
            platform::activate_app_by_pid(pid);
        }
        (None, None) => {}
    }

    // 6) 轮询等待目标真正到前台再粘贴（替代固定 120ms 死等，根治偶发「粘不上」）。
    wait_until_foreground(target.pid, Duration::from_millis(450));

    // 7) Cmd/Ctrl+V —— 无论 'v' 是否出错都释放修饰键，避免 Cmd/Ctrl 卡在按下态。
    let modifier = if cfg!(target_os = "macos") {
        Key::Meta
    } else {
        Key::Control
    };
    fn do_paste(modifier: Key) -> Result<(), String> {
        let mut e =
            Enigo::new(&EnigoSettings::default()).map_err(|err| format!("enigo init: {err}"))?;
        e.key(modifier, Direction::Press)
            .map_err(|err| format!("press: {err}"))?;
        // 'v' 出错也要继续释放修饰键；用 and 保留第一个错误。
        let v = e
            .key(Key::Unicode('v'), Direction::Click)
            .map_err(|err| format!("v: {err}"));
        let release = e
            .key(modifier, Direction::Release)
            .map_err(|err| format!("release: {err}"));
        v.and(release)
    }
    let result = do_paste(modifier);

    // 8) 不论成功失败都尝试还原剪贴板（粘贴失败时还原更应该）
    schedule_clipboard_restore(prev_clipboard, content, CLIPBOARD_RESTORE_DELAY_MS);

    match result {
        Ok(()) => {
            tracing::info!("inject ok");
            Ok(InjectResult {
                ok: true,
                fallback: None,
                message: None,
            })
        }
        Err(msg) => {
            tracing::warn!(error = %msg, "inject paste sim failed");
            Ok(InjectResult {
                ok: false,
                fallback: Some("clipboard".to_string()),
                message: Some(msg),
            })
        }
    }
}

#[tauri::command]
pub fn permissions_check_accessibility() -> bool {
    let trusted = permissions::is_trusted();
    tracing::info!(trusted, "permissions_check_accessibility");
    trusted
}

#[tauri::command]
pub fn permissions_request_accessibility() -> AppResult<bool> {
    let trusted = permissions::prompt_trust();
    tracing::info!(trusted, "permissions_request_accessibility");
    // macOS 不保证重复弹系统授权框，显式请求时直接打开对应设置页。
    #[cfg(target_os = "macos")]
    if !trusted {
        let status = std::process::Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .status()?;
        if !status.success() {
            return Err(AppError::Internal("无法打开辅助功能设置".into()));
        }
    }
    Ok(trusted)
}
