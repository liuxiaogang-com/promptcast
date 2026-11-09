// permissions.rs — 授权引导只暴露当前运行应用，不接收前端传入的文件路径。
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

#[derive(serde::Serialize)]
pub struct PermissionAppTarget {
    path: PathBuf,
    bundled: bool,
}

fn target_for_executable(exe: &Path) -> PermissionAppTarget {
    let bundle = exe
        .parent()
        .filter(|p| p.file_name().is_some_and(|n| n == "MacOS"))
        .and_then(Path::parent)
        .filter(|p| p.file_name().is_some_and(|n| n == "Contents"))
        .and_then(Path::parent)
        .filter(|p| p.extension().is_some_and(|e| e == "app"));
    PermissionAppTarget {
        path: bundle.unwrap_or(exe).to_path_buf(),
        bundled: bundle.is_some(),
    }
}

#[tauri::command]
pub fn permissions_app_target() -> AppResult<PermissionAppTarget> {
    Ok(target_for_executable(&std::env::current_exe()?))
}

// 同步 Tauri 命令在主线程执行，原生拖拽必须留在 AppKit 主线程。
#[tauri::command]
pub fn permissions_drag_app(window: tauri::WebviewWindow) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        if window.label() != "permissions" {
            return Err(AppError::InvalidInput("请从授权引导窗口拖拽".into()));
        }
        let target = permissions_app_target()?;
        drag::start_drag(
            &window,
            drag::DragItem::Files(vec![target.path]),
            drag::Image::Raw(include_bytes!("../../icons/128x128.png").to_vec()),
            |result, _| tracing::debug!(?result, "permission app drag finished"),
            drag::Options::default(),
        )
        .map_err(|e| AppError::Internal(format!("无法拖拽应用：{e}")))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        Err(AppError::InvalidInput("当前平台无需辅助功能授权".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_app_drags_its_running_bundle() {
        let target = target_for_executable(Path::new(
            "/Applications/Prompt Cast.app/Contents/MacOS/prompt-manager",
        ));
        assert_eq!(target.path, Path::new("/Applications/Prompt Cast.app"));
        assert!(target.bundled);
    }

    #[test]
    fn development_build_drags_actual_executable() {
        let exe = Path::new("/Users/test/project.app/target/debug/prompt-manager");
        let target = target_for_executable(exe);
        assert_eq!(target.path, exe);
        assert!(!target.bundled);
    }
}
