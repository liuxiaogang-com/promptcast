// commands/window.rs — drawer 显隐 + 共享 editor/preview/settings 窗口的复用。
//
// 重要变更：之前用 `WebviewWindowBuilder::build()` 运行时建窗在 Windows
// + Tauri 2.11 上会挂住（builder.build() 永不返回，子窗白屏）。改为
// 在 tauri.conf.json 预声明所有窗口（initially visible:false），运行期
// 只做 show/hide + 通过 webview.eval 切 hash 路由把 id 带入。
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize)]
pub struct WindowInfo {
    pub label: String,
}

/// 把单例窗口拉到前面。可选地先把 hash 切换到 `target_hash`
/// (例如 "#/editor/42")，让前端路由跳过去。
fn show_singleton(
    app: &AppHandle,
    label: &str,
    target_hash: Option<&str>,
) -> AppResult<WindowInfo> {
    let win = app.get_webview_window(label).ok_or_else(|| {
        AppError::NotFound(format!("window '{label}' not declared in tauri.conf"))
    })?;

    if let Some(hash) = target_hash {
        // hash 用 JSON 字符串字面量转义，避免反斜杠 / 引号注入。
        let escaped = serde_json::to_string(hash)
            .map_err(|e| AppError::Internal(format!("escape hash: {e}")))?;
        let js = format!(
            "if (location.hash !== {0}) {{ location.hash = {0}; }}",
            escaped
        );
        if let Err(e) = win.eval(&js) {
            tracing::warn!(label, error = %e, "eval hash failed");
        }
    }

    // 子窗要盖在抽屉之上：抽屉被钉住时是 topmost/浮层，会挡住子窗。show 之前先让它让出
    // topmost —— 否则子窗可能先渲染在仍是 topmost 的抽屉之下。让位后子窗以普通层级 show+focus
    // 即自然盖在抽屉上，无需把子窗也设 topmost（那会让编辑/设置窗永久浮在所有应用之上）。
    // set_always_on_top 幂等：未钉住时设 false 是 no-op。抽屉层级在最后一个子窗关闭时由
    // CloseRequested 处理按 DrawerPinned 真相源恢复。
    if let Some(drawer) = app.get_webview_window("drawer") {
        let _ = drawer.set_always_on_top(false);
    }

    let _ = win.show();
    let _ = win.unminimize();
    let _ = win.set_focus();

    tracing::info!(label, target_hash, "window shown");
    Ok(WindowInfo {
        label: label.to_string(),
    })
}

#[tauri::command]
pub fn window_show_drawer(app: AppHandle) -> AppResult<()> {
    if let Some(w) = app.get_webview_window("drawer") {
        // 与 summon_drawer / 热键唤起一致：先激活自己，show 后再抢前台 + 键盘焦点。
        // 否则前端从后台态调用时（理论上）会和热键路径有同样的「抢不到焦点」问题。
        crate::platform::activate_self();
        let _ = w.show();
        crate::platform::make_key(&w);
    }
    Ok(())
}

#[tauri::command]
pub fn window_hide_drawer(app: AppHandle) -> AppResult<()> {
    if let Some(w) = app.get_webview_window("drawer") {
        let _ = w.hide();
    }
    Ok(())
}

#[tauri::command]
pub fn window_set_pin(app: AppHandle, pinned: bool) -> AppResult<()> {
    if let Some(w) = app.get_webview_window("drawer") {
        let _ = w.set_always_on_top(pinned);
    }
    // 记录 pin 状态：失焦自动隐藏逻辑据此判断是否保留抽屉。
    app.state::<crate::DrawerPinned>()
        .0
        .store(pinned, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// 抽屉上的模态弹窗开/关。打开时让抽屉置顶且失焦不自动隐藏；关闭时把置顶恢复成用户的 pin 状态。
/// 与 window_set_pin 相互独立：不改 DrawerPinned，因此不会污染前端 pin 按钮高亮。
#[tauri::command]
pub fn window_set_modal_open(app: AppHandle, open: bool) -> AppResult<()> {
    if let Some(w) = app.get_webview_window("drawer") {
        if open {
            let _ = w.set_always_on_top(true);
        } else {
            // 关闭弹窗：层级回落到用户原本的 pin 状态，不强行取消用户的钉住。
            let pinned = app
                .state::<crate::DrawerPinned>()
                .0
                .load(std::sync::atomic::Ordering::Relaxed);
            let _ = w.set_always_on_top(pinned);
        }
    }
    app.state::<crate::DrawerModalOpen>()
        .0
        .store(open, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn window_open_preview(app: AppHandle, id: i64) -> AppResult<WindowInfo> {
    let hash = format!("#/preview/{id}");
    show_singleton(&app, "preview", Some(&hash))
}

#[tauri::command]
pub fn window_open_editor(app: AppHandle, id: Option<i64>) -> AppResult<WindowInfo> {
    tracing::info!(?id, "window_open_editor invoked");
    let hash = match id {
        Some(i) => format!("#/editor/{i}"),
        None => "#/editor".to_string(),
    };
    show_singleton(&app, "editor", Some(&hash))
}

#[tauri::command]
pub fn window_open_settings(app: AppHandle) -> AppResult<WindowInfo> {
    show_singleton(&app, "settings", Some("#/settings"))
}

#[tauri::command]
pub fn window_open_permissions(app: AppHandle) -> AppResult<WindowInfo> {
    #[cfg(target_os = "macos")]
    {
        crate::platform::activate_self();
        show_singleton(&app, "permissions", Some("#/permissions"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err(AppError::InvalidInput("当前平台无需辅助功能授权".into()))
    }
}
