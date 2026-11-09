// platform — 跨平台窗口与焦点封装。
use tauri::WebviewWindow;

pub mod permissions;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub fn apply_panel_style(_window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    macos::apply_panel_style(_window);
    #[cfg(target_os = "windows")]
    windows::apply(_window);
}

/// 启动期一次性调用：隐藏 dock 图标 / 启用 tool window 等。
pub fn init_app_chrome() {
    #[cfg(target_os = "macos")]
    macos::set_accessory_policy();
}

/// 把我们自己拉到前台。
pub fn activate_self() {
    #[cfg(target_os = "macos")]
    macos::activate_self();
}

/// 让窗口成为 key window 接收键盘事件，并真正拉到前台。
pub fn make_key(_window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    macos::make_key_and_order_front(_window);

    // Windows: 从后台进程 set_focus() 受前台锁限制（只闪任务栏）。
    // 必须用 AttachThreadInput 抢前台后再 SetFocus，否则唤起后抽屉拿不到键盘焦点。
    #[cfg(target_os = "windows")]
    {
        if let Ok(h) = _window.hwnd() {
            windows::activate_self_hwnd(h.0 as isize);
        }
    }
}

/// 返回当前 frontmost / foreground app 的 PID。
pub fn frontmost_app_pid() -> Option<i32> {
    #[cfg(target_os = "macos")]
    {
        return macos::frontmost_app_pid();
    }
    #[cfg(target_os = "windows")]
    {
        return windows::foreground_pid();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// 返回当前前台窗口的句柄（macOS 上无意义，返回 None）。
/// Windows 上比 PID 更准 —— 多窗口应用的 PID 同名但 HWND 各异。
pub fn frontmost_window_handle() -> Option<isize> {
    #[cfg(target_os = "windows")]
    {
        return windows::foreground_hwnd();
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// 通用剪贴板的「变更计数 / 序列号」。轮询它判断是否有新复制；
/// macOS = NSPasteboard.changeCount，Windows = GetClipboardSequenceNumber。
pub fn clipboard_change_count() -> i64 {
    #[cfg(target_os = "macos")]
    {
        return macos::clipboard_change_count();
    }
    #[cfg(target_os = "windows")]
    {
        return windows::clipboard_sequence();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        0
    }
}

/// 把指定 PID 的进程拉到前台。
pub fn activate_app_by_pid(_pid: i32) -> bool {
    #[cfg(target_os = "macos")]
    {
        return macos::activate_app_by_pid(_pid);
    }
    #[cfg(target_os = "windows")]
    {
        return windows::activate_pid(_pid);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// 通过窗口句柄激活；macOS 上 fallback 到 PID 路径。
pub fn activate_window_by_handle(_handle: isize, _pid_fallback: Option<i32>) -> bool {
    #[cfg(target_os = "windows")]
    {
        if windows::activate_hwnd(_handle) {
            return true;
        }
        if let Some(pid) = _pid_fallback {
            return windows::activate_pid(pid);
        }
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(pid) = _pid_fallback {
            return macos::activate_app_by_pid(pid);
        }
        return false;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
pub fn paste_shortcut() -> Result<(), String> {
    macos::paste_shortcut()
}
