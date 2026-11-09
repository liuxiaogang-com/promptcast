// platform/macos.rs — macOS 平台层。
//
// 思路修正（原来的 nonactivatingPanel 方案是错的）：
//   - macOS 的键盘事件路由按"当前激活的 NSApp"分发。NSPanel + nonactivatingPanel
//     虽然让面板成为 key window，但 NSApp 没激活时键盘事件依然去浏览器。
//   - 正确做法（Raycast 同款）:
//       1. 启动时设 NSApplicationActivationPolicyAccessory → 不显示 dock 图标
//          但允许激活成 frontmost。
//       2. 全局快捷键按下时：
//          a) 用 NSWorkspace 记录当前 frontmost PID
//          b) NSApp.activateIgnoringOtherApps:YES → 我们成为 frontmost
//          c) drawer.makeKeyAndOrderFront → 键盘进抽屉
//       3. 注入时：hide drawer → 用 NSRunningApplication 激活之前那个 PID →
//          sleep → enigo 模拟 Cmd+V。
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use tauri::WebviewWindow;

const NS_WINDOW_STYLE_MASK_BORDERLESS: u64 = 0;
const NS_WINDOW_STYLE_MASK_RESIZABLE: u64 = 1 << 3;

const NS_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES: u64 = 1 << 0;
const NS_COLLECTION_BEHAVIOR_FULLSCREEN_AUXILIARY: u64 = 1 << 8;

const NS_APPLICATION_ACTIVATION_POLICY_ACCESSORY: i64 = 1;
const NS_APPLICATION_ACTIVATE_IGNORING_OTHER_APPS: u64 = 1 << 1;

/// 启动时调一次：NSApp 不显示 dock 图标，但仍可激活。
pub fn set_accessory_policy() {
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            tracing::warn!("NSApplication.sharedApplication is null");
            return;
        }
        let ok: bool = msg_send![
            app,
            setActivationPolicy: NS_APPLICATION_ACTIVATION_POLICY_ACCESSORY
        ];
        tracing::info!(ok, "NSApp setActivationPolicy(Accessory)");
    }
}

/// 把我们自己的 NSApp 拉成 frontmost（键盘事件就会进我们的 key window）。
pub fn activate_self() {
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let _: () = msg_send![
            app,
            activateIgnoringOtherApps: true
        ];
        tracing::info!("NSApp activateIgnoringOtherApps");
    }
}

pub fn apply_panel_style(window: &WebviewWindow) {
    let ns_window: *mut AnyObject = match window.ns_window() {
        Ok(handle) => handle as *mut AnyObject,
        Err(e) => {
            tracing::warn!("ns_window unavailable: {e}");
            return;
        }
    };
    if ns_window.is_null() {
        tracing::warn!("ns_window is null");
        return;
    }
    unsafe {
        // 注意：不再用 nonactivatingPanel —— 那会让 NSApp 不激活，键盘事件
        // 直接被路由到外部的浏览器。
        let mask: u64 = NS_WINDOW_STYLE_MASK_BORDERLESS | NS_WINDOW_STYLE_MASK_RESIZABLE;
        let _: () = msg_send![ns_window, setStyleMask: mask];

        let behavior: u64 = NS_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES
            | NS_COLLECTION_BEHAVIOR_FULLSCREEN_AUXILIARY;
        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];

        let _: () = msg_send![ns_window, setHidesOnDeactivate: false];
        let _: () = msg_send![ns_window, setMovable: true];
        let _: () = msg_send![ns_window, setMovableByWindowBackground: true];
        // 窗口层级（浮动/正常）唯一由 window_set_pin → set_always_on_top 拥有。
        // 这里不再手写 setLevel —— 否则 pin/unpin 会与之争夺、导致取消钉住后层级漂移。

        // 投影：透明 + 圆角窗需要显式开启并刷新，否则窗口边缘很「突兀」。
        // macOS 会按内容的不透明轮廓（圆角卡片）生成对应的圆角阴影。
        let _: () = msg_send![ns_window, setHasShadow: true];
        let _: () = msg_send![ns_window, invalidateShadow];
    }
    tracing::info!("macOS window style applied to {}", window.label());
}

pub fn make_key_and_order_front(window: &WebviewWindow) {
    let ns_window: *mut AnyObject = match window.ns_window() {
        Ok(h) => h as *mut AnyObject,
        Err(e) => {
            tracing::warn!("ns_window err: {e}");
            return;
        }
    };
    if ns_window.is_null() {
        return;
    }
    unsafe {
        let nil: *const AnyObject = std::ptr::null();
        let _: () = msg_send![ns_window, makeKeyAndOrderFront: nil];
    }
    tracing::info!("makeKeyAndOrderFront on {}", window.label());
}

pub fn frontmost_app_pid() -> Option<i32> {
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            tracing::warn!("NSWorkspace.sharedWorkspace is null");
            return None;
        }
        let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
        if app.is_null() {
            tracing::warn!("frontmostApplication is null");
            return None;
        }
        let pid: i32 = msg_send![app, processIdentifier];
        if pid <= 0 {
            tracing::warn!(pid, "frontmost pid <= 0");
            return None;
        }
        Some(pid)
    }
}

/// 通用剪贴板的变更计数（macOS 没有变化事件，靠轮询这个计数器判断是否有新复制）。
pub fn clipboard_change_count() -> i64 {
    unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb.is_null() {
            return 0;
        }
        let n: i64 = msg_send![pb, changeCount];
        n
    }
}

pub fn activate_app_by_pid(pid: i32) -> bool {
    unsafe {
        let app: *mut AnyObject = msg_send![
            class!(NSRunningApplication),
            runningApplicationWithProcessIdentifier: pid
        ];
        if app.is_null() {
            tracing::warn!(pid, "NSRunningApplication for pid not found");
            return false;
        }
        let opts: u64 = NS_APPLICATION_ACTIVATE_IGNORING_OTHER_APPS;
        let activated: bool = msg_send![app, activateWithOptions: opts];
        tracing::info!(pid, activated, "activate_app_by_pid result");
        activated
    }
}

// V 的两个事件都显式携带 Command，避免依赖异步 modifier 状态传播。
const PASTE_KEYCODE: u16 = 0x09;
const PASTE_FLAGS: u64 = 1 << 20;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventCreateKeyboardEvent(
        source: *const std::ffi::c_void,
        key: u16,
        down: bool,
    ) -> *mut std::ffi::c_void;
    fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(value: *const std::ffi::c_void);
}

pub fn paste_shortcut() -> Result<(), String> {
    unsafe {
        let down = create_paste_event(true);
        let up = create_paste_event(false);
        if down.is_null() || up.is_null() {
            if !down.is_null() {
                CFRelease(down);
            }
            if !up.is_null() {
                CFRelease(up);
            }
            return Err("无法创建粘贴按键事件 · 已复制到剪贴板".into());
        }
        CGEventPost(0, down);
        CGEventPost(0, up);
        CFRelease(down);
        CFRelease(up);
    }
    Ok(())
}

unsafe fn create_paste_event(down: bool) -> *mut std::ffi::c_void {
    let event = CGEventCreateKeyboardEvent(std::ptr::null(), PASTE_KEYCODE, down);
    if !event.is_null() {
        CGEventSetFlags(event, PASTE_FLAGS);
    }
    event
}

#[cfg(test)]
mod paste_tests {
    use super::*;
    extern "C" {
        fn CGEventGetFlags(event: *const std::ffi::c_void) -> u64;
        fn CGEventGetIntegerValueField(event: *const std::ffi::c_void, field: u32) -> i64;
    }
    #[test]
    fn both_paste_events_carry_command_and_v_keycode() {
        for down in [true, false] {
            unsafe {
                let event = create_paste_event(down);
                assert!(!event.is_null());
                assert_eq!(CGEventGetFlags(event), PASTE_FLAGS);
                assert_eq!(
                    CGEventGetIntegerValueField(event, 9),
                    i64::from(PASTE_KEYCODE)
                );
                CFRelease(event);
            }
        }
    }
}
