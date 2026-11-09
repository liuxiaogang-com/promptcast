// lib.rs — Tauri 入口；保持薄。
mod commands;
mod db;
mod error;
mod events;
mod logging;
mod models;
mod platform;
mod sync;
mod update;

use parking_lot::Mutex;
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutEvent, ShortcutState};

use crate::db::DbState;

/// 跟踪「按下全局快捷键的瞬间」的目标窗口。
/// - pid: 跨平台都用得上（macOS 用它 NSRunningApplication 激活）
/// - hwnd: 仅 Windows 用，比 PID 更准（多窗口应用）
/// 注入时优先用 hwnd，再 fallback 到 pid。
#[derive(Default, Clone, Copy)]
pub struct FrontmostTarget {
    pub pid: Option<i32>,
    pub hwnd: Option<isize>,
}
pub struct LastFrontmost(pub Mutex<FrontmostTarget>);

/// 抽屉是否被「钉住」（钉住时失焦不自动隐藏）。由 window_set_pin 写。
pub struct DrawerPinned(pub std::sync::atomic::AtomicBool);

/// 抽屉上是否有模态弹窗打开（如更新提示）。打开时失焦也不自动隐藏，避免点别处把弹窗带走。
/// 与用户的「钉住」相互独立：不污染 DrawerPinned / 前端 pin 按钮高亮。由 window_set_modal_open 写。
pub struct DrawerModalOpen(pub std::sync::atomic::AtomicBool);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例必须第一个注册：第二次启动时唤起已有抽屉而非起新进程。
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || summon_drawer(&app2));
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let app_data = app.path().app_data_dir().expect("app_data_dir");
            let log_dir = app_data.join("logs");
            let guard = logging::init(&log_dir);
            app.manage(LoggingGuard(guard));
            tracing::info!("PromptCast starting up");

            // 隐藏 dock 图标但允许激活（必须先于窗口设置）
            crate::platform::init_app_chrome();

            let db_path = app_data.join("prompt_hub.sqlite");
            let state = DbState::open(&db_path).expect("init db");
            app.manage(state);
            app.manage(LastFrontmost(Mutex::new(FrontmostTarget::default())));
            app.manage(DrawerPinned(std::sync::atomic::AtomicBool::new(false)));
            app.manage(DrawerModalOpen(std::sync::atomic::AtomicBool::new(false)));
            app.manage(crate::sync::SyncRuntime::default());
            // 重启后恢复会话：钥匙串有 refresh ⇒ 已登录；email 从 settings 取回供 UI 显示。
            // access token 不落盘，首次同步会用 refresh 换取。
            let auth_transition_pending = crate::db::settings::get_raw(
                &app.state::<DbState>().0.lock(),
                crate::sync::AUTH_TRANSITION_KEY,
            )
            .map(|v| v.as_deref() == Some("1"))
            .unwrap_or(true);
            if !auth_transition_pending && crate::sync::load_refresh().is_some() {
                let db = app.state::<DbState>();
                let email = {
                    let conn = db.0.lock();
                    crate::db::settings::get_raw(&conn, "sync_user_email")
                        .ok()
                        .flatten()
                };
                if let Some(email) = email.filter(|e| !e.is_empty()) {
                    *app.state::<crate::sync::SyncRuntime>().email.lock() = Some(email);
                }
            }

            // 列出所有从 tauri.conf.json 预声明的窗口（用于确认 conf 是否生效）
            let labels: Vec<String> = app.webview_windows().keys().cloned().collect();
            tracing::info!(?labels, "webview windows at startup");

            if let Some(drawer) = app.get_webview_window("drawer") {
                crate::platform::apply_panel_style(&drawer);
                #[cfg(debug_assertions)]
                {
                    // drawer.open_devtools();
                }
                // 失焦自动隐藏（钉住时不隐藏）—— 点开别的 App / 打开编辑窗即收起，像 Raycast。
                let app_h = app.handle().clone();
                let dr = drawer.clone();
                // 启动初期终端仍在前台，抽屉会立刻收到一次 Focused(false)。改用「首次真正聚焦后
                // 才生效」取代 1.2s 墙钟启发式：慢机冷启动 >1.2s 不再误隐藏，也不吞早期真实失焦。
                let ever_focused = std::sync::atomic::AtomicBool::new(false);
                drawer.on_window_event(move |event| match event {
                    tauri::WindowEvent::Focused(true) => {
                        ever_focused.store(true, std::sync::atomic::Ordering::Relaxed);
                        // 获焦时触发一次同步（拉取别的设备的更新）。
                        if let Some(rt) = app_h.try_state::<crate::sync::SyncRuntime>() {
                            rt.wake.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    tauri::WindowEvent::Focused(false) => {
                        // 还没真正聚焦过（启动阶段那次失焦）不隐藏。
                        if !ever_focused.load(std::sync::atomic::Ordering::Relaxed) {
                            return;
                        }
                        let pinned = app_h
                            .state::<DrawerPinned>()
                            .0
                            .load(std::sync::atomic::Ordering::Relaxed);
                        if pinned {
                            return;
                        }
                        // 抽屉上有模态弹窗（更新提示等）时，失焦也不收起，避免点别处把弹窗带走。
                        if app_h
                            .state::<DrawerModalOpen>()
                            .0
                            .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            return;
                        }
                        // 焦点落到我们自己其它窗口（debug 的 devtools / editor / preview /
                        // settings）时不隐藏 —— 否则 devtools 抢焦点会让抽屉「一出现就消失」。
                        // 只有焦点真的切到别的 App（pid 不同）才收起，像 Raycast。
                        let our_pid = std::process::id() as i32;
                        if crate::platform::frontmost_app_pid() == Some(our_pid) {
                            return;
                        }
                        let _ = dr.hide();
                    }
                    _ => {}
                });
            } else {
                tracing::warn!("drawer window not found at startup");
            }

            // editor/preview/settings 是 tauri.conf 预声明的单例窗口，靠 show/hide 复用。
            // 关闭时若真的销毁，show_singleton 之后会 NotFound → 开一次就再也打不开。
            // 因此拦截 CloseRequested：阻止默认关闭，改为隐藏（覆盖前端 .close() 与原生关闭按钮）。
            for label in ["editor", "preview", "settings", "permissions", "changelog"] {
                if let Some(win) = app.get_webview_window(label) {
                    let hide_target = win.clone();
                    let app_h = app.handle().clone();
                    win.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            api.prevent_close();
                            let _ = hide_target.hide();
                            tracing::info!(
                                label = hide_target.label(),
                                "close intercepted -> hidden"
                            );

                            // 最后一个子窗关掉时，按 DrawerPinned 真相源恢复抽屉层级、并把焦点拉回抽屉。
                            // 用「其它子窗是否仍可见」从实际状态推导，而非事件计数 —— 计数会因
                            // is_visible() 偶发 Err / CloseRequested 双发而漂移。排除刚 hide 的这个窗，
                            // 规避 Windows 上 hide() 异步未即时生效的误判。
                            // makeKeyAndOrderFront(macOS) 是主线程 AppKit API，窗口事件不保证在主线程，
                            // 必须 dispatch 到主线程，否则 UB。
                            let closing = hide_target.label().to_string();
                            let app2 = app_h.clone();
                            let _ = app_h.run_on_main_thread(move || {
                                let any_other_visible =
                                    ["editor", "preview", "settings", "permissions", "changelog"]
                                        .iter()
                                        .any(|&l| {
                                            l != closing.as_str()
                                                && app2
                                                    .get_webview_window(l)
                                                    .and_then(|w| w.is_visible().ok())
                                                    .unwrap_or(false)
                                        });
                                if any_other_visible {
                                    return;
                                }
                                if let Some(drawer) = app2.get_webview_window("drawer") {
                                    let pinned = app2
                                        .state::<DrawerPinned>()
                                        .0
                                        .load(std::sync::atomic::Ordering::Relaxed);
                                    let _ = drawer.set_always_on_top(pinned);
                                    // 子窗清空后焦点回抽屉（仍可见时），避免落到桌面。
                                    if drawer.is_visible().unwrap_or(false) {
                                        crate::platform::make_key(&drawer);
                                    }
                                }
                            });
                        }
                    });
                }
            }

            let settings = crate::db::settings::get_all(&app.state::<DbState>().0.lock()).ok();
            if let Some(s) = settings {
                if let Some(shortcut) = s.hotkey {
                    if let Err(e) = register_global_hotkey(app.handle(), &shortcut) {
                        tracing::error!(error = %e, "register startup hotkey failed");
                    }
                }
                // 启动时把持久化的 auto_start 偏好同步到系统层
                crate::commands::settings::apply_autostart(app.handle(), s.auto_start);
            }

            // ---- 系统托盘（macOS 菜单栏图标 / Windows 任务栏托盘）----
            // 本应用无 dock 图标（Accessory 策略），托盘是热键之外唯一的常驻入口；
            // 左键点击唤起抽屉，右键弹菜单。
            {
                let menu = MenuBuilder::new(app)
                    .text("show", "显示抽屉")
                    .text("settings", "设置…")
                    .separator()
                    .text("quit", "退出 PromptCast")
                    .build()?;

                let mut builder = TrayIconBuilder::with_id("main")
                    .tooltip("PromptCast")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "show" => summon_drawer(app),
                        "settings" => {
                            let _ = crate::commands::window::window_open_settings(app.clone());
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            summon_drawer(tray.app_handle());
                        }
                    });
                if let Some(icon) = app.default_window_icon() {
                    builder = builder.icon(icon.clone());
                }
                match builder.build(app) {
                    Ok(_) => tracing::info!("tray icon installed"),
                    Err(e) => tracing::error!(error = %e, "tray build failed"),
                }
            }

            // 启动显示统一在 Ready 处理，包括更新后的重启。
            let flag = crate::update::relaunch_flag_path();
            if flag.exists() {
                if let Err(e) = std::fs::remove_file(&flag) {
                    tracing::warn!(error = %e, "failed to remove relaunch flag");
                }
            }

            // ---- 剪贴板历史监听线程：轮询 changeCount，文本变化即入库 ----
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || clipboard_monitor_loop(handle));
            }

            // ---- 同步循环线程：登录+开启时，定时/获焦/手动触发跑一拍 ----
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || sync_loop(handle));
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping::ping,
            commands::window::window_open_permissions,
            commands::permissions::permissions_app_target,
            commands::permissions::permissions_drag_app,
            logging::log_record,
            logging::log_dir,
            commands::prompts::prompts_list,
            commands::prompts::prompts_get,
            commands::prompts::prompts_create,
            commands::prompts::prompts_update,
            commands::prompts::prompts_delete,
            commands::prompts::prompts_toggle_favorite,
            commands::prompts::prompts_toggle_pin,
            commands::prompts::prompts_record_use,
            commands::folders::folders_list,
            commands::folders::folders_create,
            commands::folders::folders_rename,
            commands::folders::folders_delete,
            commands::folders::folders_reorder,
            commands::tags::tags_list,
            commands::tags::tags_create,
            commands::tags::tags_rename,
            commands::tags::tags_delete,
            commands::settings::settings_get_all,
            commands::settings::settings_set,
            commands::data::data_export_json,
            commands::data::data_export_to_file,
            commands::data::data_import_json,
            commands::sites::sites_list,
            commands::sites::sites_create,
            commands::sites::sites_update,
            commands::sites::sites_delete,
            commands::sites::sites_reorder,
            commands::sites::sites_refresh_favicon,
            commands::sites::sites_open,
            commands::inject::inject_paste,
            commands::inject::inject_copy_only,
            commands::inject::permissions_check_accessibility,
            commands::inject::permissions_request_accessibility,
            commands::window::window_show_drawer,
            commands::window::window_hide_drawer,
            commands::window::window_set_pin,
            commands::window::window_set_modal_open,
            commands::window::window_open_preview,
            commands::window::window_open_editor,
            commands::window::window_open_settings,
            commands::window::window_open_changelog,
            commands::clipboard::clipboard_list,
            commands::clipboard::clipboard_delete,
            commands::clipboard::clipboard_clear,
            register_hotkey_cmd,
            unregister_hotkey_cmd,
            backend_ready,
            commands::auth::auth_register,
            commands::auth::auth_login,
            commands::auth::auth_logout,
            commands::auth::auth_status,
            commands::auth::auth_change_password,
            commands::auth::auth_delete_account,
            commands::auth::auth_forgot_password,
            commands::auth::auth_reset_password,
            commands::sync::sync_status,
            commands::sync::sync_set_enabled,
            commands::sync::sync_now,
            commands::sync::sync_get_server_url,
            commands::sync::sync_set_server_url,
            commands::update::update_check,
            commands::update::update_changelog,
            commands::update::update_download_install,
            commands::update::update_get_manifest_url,
            commands::update::update_set_manifest_url,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            tauri::RunEvent::Ready => {
                summon_drawer(app);
                #[cfg(target_os = "macos")]
                show_startup_permission_hint(app);
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => summon_drawer(app),
            _ => {}
        });
}

#[cfg(target_os = "macos")]
fn show_startup_permission_hint(app: &tauri::AppHandle) {
    if !crate::platform::permissions::is_trusted() {
        if let Err(e) = crate::commands::window::window_open_permissions(app.clone()) {
            tracing::error!(error = %e, "failed to show permission guide");
        }
    }
}

struct LoggingGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

#[tauri::command]
fn register_hotkey_cmd(app: tauri::AppHandle, shortcut: String) -> Result<(), String> {
    crate::commands::settings::set_hotkey(&app, Some(shortcut)).map_err(|e| e.to_string())
}

#[tauri::command]
fn unregister_hotkey_cmd(app: tauri::AppHandle) -> Result<(), String> {
    crate::commands::settings::set_hotkey(&app, None).map_err(|e| e.to_string())
}

/// 后端是否就绪：DbState 是否已 manage。
/// 预声明窗口的 webview 在独立进程里启动，其 mounted 的 IPC 会和 setup() 里
/// `app.manage(DbState)` 抢跑 —— 抢赢时命令报 "state not managed for db"，
/// 导致该窗口数据加载失败、卡在「加载中」。前端启动时先轮询本命令直到 true 再加载，
/// 用 try_state 永不报错（未就绪返回 false）。
#[tauri::command]
fn backend_ready(app: tauri::AppHandle) -> bool {
    app.try_state::<DbState>().is_some()
}

/// 同步循环（后台线程）。登录 + 开启时，每 ~60s / 获焦 / 手动触发跑一拍。
/// 网络/认证失败只记日志、emit error 状态，下拍重试（保游标、不丢 dirty）。
fn sync_loop(app: tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};
    std::thread::sleep(Duration::from_secs(2));
    let mut last_tick = Instant::now();
    let mut fails: u32 = 0;
    let mut backoff_until: Option<Instant> = None;
    tracing::info!("sync loop started");
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let rt = match app.try_state::<crate::sync::SyncRuntime>() {
            Some(r) => r,
            None => continue,
        };
        let woke = rt.wake.swap(false, Ordering::Relaxed);
        // 退避期内：非手动唤起就跳过（手动「立即同步」可越过退避强制重试）。
        if let Some(until) = backoff_until {
            if Instant::now() < until && !woke {
                continue;
            }
        }
        let periodic = last_tick.elapsed() >= Duration::from_secs(60);
        if !woke && !periodic {
            continue;
        }
        last_tick = Instant::now();
        if rt.busy.swap(true, Ordering::Relaxed) {
            continue; // 已在跑
        }
        crate::sync::emit_status(&app, "syncing", None);
        match crate::sync::engine::sync_once(&app) {
            Ok(o) => {
                if o.pushed > 0 || o.pulled > 0 {
                    tracing::info!(pushed = o.pushed, pulled = o.pulled, "sync ok");
                }
                fails = 0;
                backoff_until = None;
                crate::sync::emit_status(&app, "idle", None);
            }
            Err(e) => {
                fails = fails.saturating_add(1);
                // 指数退避（2,4,8,…封顶 300s）+ 抖动，避免网络抖动时刷屏/打满。
                let base = 2u64.saturating_pow(fails.min(6)).min(300);
                let jitter = sync_jitter_secs();
                backoff_until = Some(Instant::now() + Duration::from_secs(base + jitter));
                let msg = e.to_string();
                let state = if msg.contains("network") {
                    "offline"
                } else {
                    "error"
                };
                tracing::warn!(error = %e, fails, backoff = base, "sync cycle failed; backing off");
                crate::sync::emit_status(&app, state, Some(msg));
            }
        }
        rt.busy.store(false, Ordering::Relaxed);
    }
}

/// 退避抖动 0..=3s（用系统时间纳秒取低位，避免引入 rand 依赖）。
fn sync_jitter_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.subsec_nanos() as u64) % 4)
        .unwrap_or(0)
}

/// 剪贴板历史监听循环（后台线程）。macOS 无变化事件，靠轮询 changeCount；
/// 检测到变化就读文本入库。非文本（图片/附件）静默跳过；跳过 app 自己写入的内容。
fn clipboard_monitor_loop(app: tauri::AppHandle) {
    use std::time::Duration;
    let mut last_count = crate::platform::clipboard_change_count();
    tracing::info!("clipboard monitor started");
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let count = crate::platform::clipboard_change_count();
        if count == last_count {
            continue;
        }
        last_count = count;

        // 读设置（启用开关 + 上限）；DbState 不在就用默认值。
        let (enabled, limit) = match app.try_state::<DbState>() {
            Some(state) => {
                let conn = state.0.lock();
                match crate::db::settings::get_all(&conn) {
                    Ok(s) => (
                        s.clipboard_history_enabled,
                        s.clipboard_history_limit as i64,
                    ),
                    Err(_) => (true, 500),
                }
            }
            None => (true, 500),
        };
        if !enabled {
            continue;
        }

        // 读剪贴板文本；非文本（图片/附件）get_text 失败 → 静默跳过。
        let text = match arboard::Clipboard::new()
            .ok()
            .and_then(|mut c| c.get_text().ok())
        {
            Some(t) => t,
            None => continue,
        };
        if text.trim().is_empty() {
            continue;
        }
        // 跳过我们自己写入的内容（注入 / 复制 / 还原）。
        if crate::commands::inject::is_recent_self_copy(&text) {
            continue;
        }

        if let Some(state) = app.try_state::<DbState>() {
            let inserted = {
                let conn = state.0.lock();
                crate::db::clipboard::insert(&conn, &text, limit).unwrap_or(false)
            };
            if inserted {
                crate::events::emit_clipboard_changed(&app);
                tracing::info!(chars = text.chars().count(), "clipboard history recorded");
            }
        }
    }
}

/// 把抽屉唤起到前台并成为 key window（托盘左键 / 菜单「显示抽屉」复用）。
/// 与全局热键一致：先记录当前 frontmost 作为注入目标，再激活自己。
fn summon_drawer(app: &tauri::AppHandle) {
    let drawer = match app.get_webview_window("drawer") {
        Some(w) => w,
        None => return,
    };
    let our_pid = std::process::id() as i32;
    if let Some(pid) = crate::platform::frontmost_app_pid() {
        if pid != our_pid {
            if let Some(state) = app.try_state::<LastFrontmost>() {
                *state.0.lock() = FrontmostTarget {
                    pid: Some(pid),
                    hwnd: crate::platform::frontmost_window_handle(),
                };
            }
        }
    }
    crate::platform::activate_self();
    let _ = drawer.show();
    crate::platform::make_key(&drawer);
    tracing::info!("summon_drawer: shown + made key");
}

fn register_global_hotkey(app: &tauri::AppHandle, shortcut: &str) -> Result<(), String> {
    let app_clone = app.clone();
    let our_pid = std::process::id() as i32;
    let res = app
        .global_shortcut()
        .on_shortcut(shortcut, move |_app, _sc, ev: ShortcutEvent| {
            if ev.state() != ShortcutState::Pressed {
                return;
            }

            let drawer = match app_clone.get_webview_window("drawer") {
                Some(w) => w,
                None => return,
            };
            let visible = drawer.is_visible().unwrap_or(false);

            tracing::info!(visible, "hotkey pressed");

            if visible {
                let _ = drawer.hide();
                tracing::info!("hotkey: drawer hidden");
                return;
            }

            // 1) 记录当前 frontmost（必须在我们激活之前 — 那时候我们的
            //    PID 还不是 frontmost）。
            let pid_now = crate::platform::frontmost_app_pid();
            let hwnd_now = crate::platform::frontmost_window_handle();
            match pid_now {
                Some(pid) if pid != our_pid => {
                    if let Some(state) = app_clone.try_state::<LastFrontmost>() {
                        *state.0.lock() = FrontmostTarget {
                            pid: Some(pid),
                            hwnd: hwnd_now,
                        };
                        tracing::info!(
                            target_pid = pid,
                            target_hwnd = hwnd_now.unwrap_or(0),
                            "saved last frontmost target"
                        );
                    }
                }
                Some(pid) => {
                    tracing::warn!(pid, "frontmost is self; keeping previous saved target");
                }
                None => {
                    tracing::warn!("no frontmost app detected");
                }
            }

            // 2)+3) NSApp 激活 + 窗口 show + makeKeyAndOrderFront 都是 AppKit 主线程 API；
            //    热键回调在插件线程，必须 dispatch 到主线程，否则是 UB（偶发崩溃/卡死）。
            let drawer_main = drawer.clone();
            let _ = app_clone.run_on_main_thread(move || {
                crate::platform::activate_self();
                let _ = drawer_main.show();
                crate::platform::make_key(&drawer_main);
                tracing::info!("hotkey: activated self + drawer shown + made key (main thread)");
            });
        });
    res.map_err(|e| format!("快捷键注册失败，可能已被其他应用占用：{e}"))?;
    tracing::info!(shortcut, "global hotkey registered");
    Ok(())
}
