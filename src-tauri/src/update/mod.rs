// update — 轻量自建更新器（离线优先 app 的“查更新”能力，与同步无关）。
//
// 设计：托管一个 JSON 清单（用户放 CNB），里面写版本号 / 标题 / 说明 / 分平台下载链接
// （可选 sha256）。客户端拉清单 → 跟自身版本比 semver → 有新版就让前端弹窗 → 用户点更新
// 后由本模块下载安装包（带进度 + 可选 sha256 校验）→ 拉起安装器（Windows 直接跑 setup/msi，
// macOS `open` 挂载 dmg）。不做静默自动替换（那需要 minisign 签名，见 plan）。
//
// 安全约束：
//  - 下载链接必须 https（要执行的是安装器，明文下载可被 MITM 注入）。
//  - 安装链接/sha 不经过前端：前端只拿到展示信息，真正下载时本模块按当前平台重新读清单，
//    避免“前端被骗传任意 URL → 下载执行任意二进制”。
//  - sha256 在清单里给了就强制校验，不匹配即删文件报错；没给则跳过（仅记日志）。
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

const MANIFEST_URL_KEY: &str = "update_manifest_url";
const PROGRESS_EVENT: &str = "update-progress";

/// 固定入口可通过 HTTPS 重定向换源，不再要求客户端重新发版。
pub const DEFAULT_MANIFEST_URL: &str = "https://go.liuxiaogang.com/promptcast-update";
pub const CHANGELOG_URL: &str = "https://go.liuxiaogang.com/promptcast-changelog";

// 同一时刻只允许一个下载在跑：清单/资产下载是固定临时路径，且更新窗在 drawer 与 settings
// 两个 webview 各挂一份；两窗并发下载会抢同一个文件、进度也会串。compare_exchange 占坑。
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
pub enum UpdateError {
    NotConfigured,
    Busy,
    Network(String),
    Parse(String),
    NoAsset(String),
    Insecure(String),
    Checksum(String),
    Io(String),
    Launch(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::NotConfigured => write!(f, "未配置更新地址"),
            UpdateError::Busy => write!(f, "更新下载已在进行中"),
            UpdateError::Network(s) => write!(f, "网络错误: {s}"),
            UpdateError::Parse(s) => write!(f, "清单解析失败: {s}"),
            UpdateError::NoAsset(s) => write!(f, "该平台暂无安装包: {s}"),
            UpdateError::Insecure(s) => {
                write!(f, "链接必须为 https（本地测试可用 localhost）: {s}")
            }
            UpdateError::Checksum(s) => write!(f, "校验失败（文件可能被篡改或损坏）: {s}"),
            UpdateError::Io(s) => write!(f, "写入失败: {s}"),
            UpdateError::Launch(s) => write!(f, "拉起安装器失败: {s}"),
        }
    }
}

/// 链接是否可接受：真实网络一律 https（要下载执行的是安装器，明文可被 MITM 换成恶意二进制）；
/// 仅放行 localhost/127.0.0.1 的 http，便于本地端到端测试。清单 URL 与资产 URL 共用此判据。
/// 按解析出的 host 判定，而非字符串前缀 —— 否则 `http://localhost.evil.com` 会被前缀匹配蒙混过关。
fn is_secure_or_local(u: &str) -> bool {
    match url::Url::parse(u) {
        Ok(p) => {
            p.scheme() == "https"
                || (p.scheme() == "http"
                    && matches!(p.host_str(), Some("localhost") | Some("127.0.0.1")))
        }
        Err(_) => false,
    }
}

/// 单个平台的下载条目。
#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub url: String,
    /// 可选：十六进制 sha256；给了就强制校验。
    #[serde(default)]
    pub sha256: Option<String>,
}

/// 托管的更新清单（用户在 CNB 上维护）。
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub pub_date: Option<String>,
    /// key = `{os}-{arch}`，如 windows-x86_64 / darwin-aarch64 / darwin-x86_64。
    pub downloads: std::collections::HashMap<String, Asset>,
}

/// 返回给前端展示用（不含 url/sha —— 真正下载由后端按平台重新读清单）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub title: String,
    pub notes: String,
    pub pub_date: Option<String>,
}

/// 日志独立于更新清单；包含已安装版本和历史版本，不受更新检查结果影响。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReleaseNotes {
    pub version: String,
    pub title: String,
    pub notes: String,
    pub pub_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Changelog {
    releases: Vec<ReleaseNotes>,
}

pub fn changelog() -> Result<Vec<ReleaseNotes>, UpdateError> {
    let log: Changelog = fetch_json(CHANGELOG_URL)?;
    Ok(log.releases)
}

/// 当前 app 版本（编译期来自 Cargo.toml）。
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// `{os}-{arch}`，与清单 downloads 的 key 对齐。macOS 用 darwin。
pub fn platform_key() -> String {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

/// remote 是否比 current 新。优先 semver；任一解析失败回退到数字三元组比较；
/// 再不行就保守判为“不更新”（避免脏数据触发误弹窗）。
pub fn is_newer(remote: &str, current: &str) -> bool {
    let r = remote.trim().trim_start_matches('v');
    let c = current.trim().trim_start_matches('v');
    if let (Ok(rv), Ok(cv)) = (semver::Version::parse(r), semver::Version::parse(c)) {
        return rv > cv;
    }
    match (parse_triple(r), parse_triple(c)) {
        (Some(rt), Some(ct)) => rt > ct,
        _ => false,
    }
}

fn parse_triple(s: &str) -> Option<(u64, u64, u64)> {
    // 取主版本三段，丢掉任何 -pre / +build 后缀。
    let core = s.split(['-', '+']).next().unwrap_or(s);
    let mut it = core.split('.').map(|p| p.parse::<u64>());
    let a = it.next()?.ok()?;
    let b = it.next().transpose().ok()?.unwrap_or(0);
    let c = it.next().transpose().ok()?.unwrap_or(0);
    Some((a, b, c))
}

/// 返回清单地址：DB 显式存了非空地址则以 DB 为准（仅供本地测试的 IPC 后门，UI 不暴露），
/// 否则用预制常量。即「自动生成、用户改不了」——前端没有任何入口能写它。
pub fn get_manifest_url(conn: &rusqlite::Connection) -> String {
    crate::db::settings::get_raw(conn, MANIFEST_URL_KEY)
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_MANIFEST_URL.to_string())
}

pub fn set_manifest_url(conn: &rusqlite::Connection, url: &str) -> crate::error::AppResult<()> {
    let u = url.trim();
    // 空 = 未配置（允许）；非空必须 https（或 localhost），不然存进去启动自动查会拉明文。
    if !u.is_empty() && !is_secure_or_local(u) {
        return Err(crate::error::AppError::InvalidInput(
            UpdateError::Insecure(u.to_string()).to_string(),
        ));
    }
    crate::db::settings::set_raw(conn, MANIFEST_URL_KEY, u)
}

fn http_client(timeout: Duration) -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent(format!("PromptCast/{}", current_version()))
        .timeout(timeout)
        // 下载执行型安装器：禁止 https→http 降级跳转（攻击者发个合规 https 链接 302 到
        // http 明文也能绕过初始 https 校验）。只跟随 https 跳转，并封顶跳数防环。
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            // 非 https 跳转（降级）或跳数超限 → 停（3xx 会被后续 is_success 判错）。
            if attempt.url().scheme() != "https" || attempt.previous().len() >= 10 {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .expect("reqwest client")
}

fn fetch_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, UpdateError> {
    // 清单是整条信任链的根：它给出版本号和各平台资产 URL。它自己也必须走 https，
    // 否则明文清单被 MITM 换掉，攻击者可同时伪造版本号 + 自带 sha256 的恶意资产链接。
    if !is_secure_or_local(url) {
        return Err(UpdateError::Insecure(url.to_string()));
    }
    let resp = http_client(Duration::from_secs(20))
        .get(url)
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .send()
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(UpdateError::Network(format!("http {}", resp.status())));
    }
    resp.json::<T>()
        .map_err(|e| UpdateError::Parse(e.to_string()))
}

fn fetch_manifest(url: &str) -> Result<Manifest, UpdateError> {
    fetch_json(url)
}

/// 查更新：读清单 → 比版本。无新版 → Ok(None)，本平台无包返回明确错误。
/// 入参是已解析好的清单地址；调用方负责在拿地址后**先释放 DbState 锁再调本函数**，
/// 否则网络往返期间整库被锁，剪贴板/同步/列表全卡（单连接架构）。
pub fn check(url: &str) -> Result<Option<UpdateInfo>, UpdateError> {
    let m = fetch_manifest(url)?;
    let cur = current_version();
    if !is_newer(&m.version, cur) {
        return Ok(None);
    }
    // 本平台没有对应安装包就不提示（提示了也没法装）。
    if !m.downloads.contains_key(&platform_key()) {
        tracing::info!(
            platform = platform_key(),
            "update available but no asset for this platform"
        );
        return Err(UpdateError::NoAsset(platform_key()));
    }
    Ok(Some(UpdateInfo {
        version: m.version,
        current: cur.to_string(),
        title: m.title,
        notes: m.notes,
        pub_date: m.pub_date,
    }))
}

fn filename_from_url(url: &str) -> String {
    let name = url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|mut s| s.next_back().map(|x| x.to_string()))
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "installer".to_string());
    // 防路径穿越：只留文件名安全字符。
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect()
}

fn emit_progress(app: &AppHandle, downloaded: u64, total: Option<u64>) {
    let _ = app.emit(
        PROGRESS_EVENT,
        serde_json::json!({ "downloaded": downloaded, "total": total }),
    );
}

/// 下载本平台安装包（带进度 + 可选 sha256 校验），返回落盘路径。
fn download_asset(app: &AppHandle, asset: &Asset) -> Result<PathBuf, UpdateError> {
    // 要执行的是安装器：真实网络强制 https（localhost 例外，便于本地测试），
    // 明文下载可被中间人替换成恶意二进制。
    if !is_secure_or_local(&asset.url) {
        return Err(UpdateError::Insecure(asset.url.clone()));
    }
    let dir = std::env::temp_dir().join("promptcast-update");
    std::fs::create_dir_all(&dir).map_err(|e| UpdateError::Io(e.to_string()))?;
    let path = dir.join(filename_from_url(&asset.url));

    let mut resp = http_client(Duration::from_secs(300))
        .get(&asset.url)
        .send()
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(UpdateError::Network(format!("http {}", resp.status())));
    }
    let total = resp.content_length();
    let mut file = std::fs::File::create(&path).map_err(|e| UpdateError::Io(e.to_string()))?;
    let mut hasher = Sha256::new();
    // 只在清单给了「像样的」sha256 时才校验：空串/占位文字/长度不对一律按“没给”处理并告警，
    // 避免一个笔误就把所有人挡在更新之外（能改清单的攻击者本来也可以直接不写 hash）。
    let want_hash = asset
        .sha256
        .as_ref()
        .map(|h| h.trim().to_lowercase())
        .filter(|h| is_valid_sha256(h));
    if want_hash.is_none() && asset.sha256.as_ref().is_some_and(|h| !h.trim().is_empty()) {
        tracing::warn!(
            "update manifest sha256 is not a valid 64-hex digest; skipping verification"
        );
    }
    let mut buf = [0u8; 65536];
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    emit_progress(app, 0, total);
    loop {
        let n = resp
            .read(&mut buf)
            .map_err(|e| UpdateError::Network(e.to_string()))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| UpdateError::Io(e.to_string()))?;
        if want_hash.is_some() {
            hasher.update(&buf[..n]);
        }
        downloaded += n as u64;
        // 约每 256KB 播报一次，避免事件刷屏。
        if downloaded - last_emit >= 262_144 {
            emit_progress(app, downloaded, total);
            last_emit = downloaded;
        }
    }
    file.flush().map_err(|e| UpdateError::Io(e.to_string()))?;
    drop(file);
    emit_progress(app, downloaded, total);

    // 连接被提前关闭时 read 返回 Ok(0)（EOF）而非错误 → 会落一个截断的安装器。
    // 服务端给了 Content-Length 就拿它兜底，截断即删文件报错（清单没 sha256 时这是唯一防线）。
    if let Some(t) = total {
        if downloaded != t {
            let _ = std::fs::remove_file(&path);
            return Err(UpdateError::Network(format!(
                "下载不完整：应为 {t} 字节，实得 {downloaded}"
            )));
        }
    }

    if let Some(want) = want_hash {
        let got = hex_lower(&hasher.finalize());
        if got != want {
            let _ = std::fs::remove_file(&path);
            return Err(UpdateError::Checksum(format!("expected {want}, got {got}")));
        }
        tracing::info!("update asset sha256 verified");
    } else {
        tracing::warn!("update asset has no sha256 in manifest; skipped integrity check");
    }
    Ok(path)
}

/// 是否是像样的 sha256：64 个十六进制字符（已小写）。
fn is_valid_sha256(h: &str) -> bool {
    h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 拉起安装器（仅作回退；正常更新走 windows_self_update）。
#[cfg(target_os = "windows")]
fn launch_installer(path: &std::path::Path) -> Result<(), UpdateError> {
    std::process::Command::new("cmd")
        .arg("/C")
        .arg("start")
        .arg("")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| UpdateError::Launch(e.to_string()))
}

/// Windows 自更新：写一个分离批处理（等本体退出 → 静默运行 NSIS 安装器 → 重启新版），
/// 启动它，然后延迟退出本体。流程与 macOS 的 `macos_self_update` 对称。
#[cfg(target_os = "windows")]
fn windows_self_update(app: &AppHandle, installer: &std::path::Path) -> Result<(), UpdateError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let dir = std::env::temp_dir().join("promptcast-update");
    std::fs::create_dir_all(&dir).map_err(|e| UpdateError::Io(e.to_string()))?;
    let script_path = dir.join("pc-selfupdate.bat");
    std::fs::write(&script_path, WINDOWS_UPDATE_SCRIPT)
        .map_err(|e| UpdateError::Io(e.to_string()))?;

    let _ = std::fs::write(relaunch_flag_path(), "1");

    let exe = std::env::current_exe().map_err(|e| UpdateError::Launch(e.to_string()))?;

    std::process::Command::new("cmd")
        .arg("/C")
        .arg(&script_path)
        .arg(installer)
        .arg(std::process::id().to_string())
        .arg(&exe)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| UpdateError::Launch(e.to_string()))?;

    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        handle.exit(0);
    });
    Ok(())
}

/// 等本体退出 → 静默 NSIS 安装 → 重启新版。用 `ping` 做延时（无窗口进程不支持 `timeout`）。
#[cfg(target_os = "windows")]
const WINDOWS_UPDATE_SCRIPT: &str = r#"@echo off
set "INSTALLER=%~1"
set "PID=%~2"
set "EXE=%~3"
set /a N=0
:wait
if %N% GEQ 100 goto install
tasklist /FI "PID eq %PID%" 2>NUL | find "%PID%" >NUL
if not errorlevel 1 (
    set /a N+=1
    ping -n 2 127.0.0.1 >NUL
    goto wait
)
:install
start /wait "" "%INSTALLER%" /S
if exist "%EXE%" start "" "%EXE%"
del "%INSTALLER%" 2>NUL
"#;

#[cfg(target_os = "macos")]
fn launch_installer(path: &std::path::Path) -> Result<(), UpdateError> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| UpdateError::Launch(e.to_string()))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn launch_installer(path: &std::path::Path) -> Result<(), UpdateError> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| UpdateError::Launch(e.to_string()))
}

/// 下载并拉起安装器。重新读清单按当前平台取链接（不信任前端传值）。
/// 入参是已解析好的清单地址；同 `check`，调用方必须先释放 DbState 锁再调（下载长达数分钟，
/// 持锁会冻结整库）。
pub fn download_and_install(
    app: &AppHandle,
    url: &str,
    expected_version: &str,
) -> Result<(), UpdateError> {
    // 占坑：已有下载在跑就直接拒，避免两窗抢同一临时文件 / 进度串台。
    if DOWNLOADING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(UpdateError::Busy);
    }
    // 守卫：任何返回路径（含 ?）都会 drop 它，复位标志，不会卡死后续下载。
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            DOWNLOADING.store(false, Ordering::SeqCst);
        }
    }
    let _guard = Guard;

    let m = fetch_manifest(url)?;
    // 清单换版后要求重新确认，不能展示 A 却安装 B。
    if m.version != expected_version || !is_newer(&m.version, current_version()) {
        return Err(UpdateError::Parse("版本信息已变化，请重新检查更新".into()));
    }
    let asset = m
        .downloads
        .get(&platform_key())
        .ok_or_else(|| UpdateError::NoAsset(platform_key()))?;
    let path = download_asset(app, asset)?;
    tracing::info!(path = %path.display(), "update downloaded");
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = current_app_bundle() {
            tracing::info!(bundle = %bundle.display(), "scheduling macOS self-update");
            return macos_self_update(app, &path, &bundle);
        }
        tracing::warn!("not inside a .app (dev build?); falling back to opening dmg");
    }
    #[cfg(target_os = "windows")]
    {
        tracing::info!("scheduling Windows silent self-update");
        return windows_self_update(app, &path);
    }
    launch_installer(&path)
}

/// 更新重启标记路径：自更新前写入，新实例启动时检测到则唤起主窗口（让用户知道已重启成功）并删除它。
pub fn relaunch_flag_path() -> PathBuf {
    std::env::temp_dir()
        .join("promptcast-update")
        .join("relaunch.flag")
}

/// 当前运行的可执行文件所在的 `.app` 包路径（macOS）。开发构建（裸二进制）返回 None。
#[cfg(target_os = "macos")]
fn current_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .map(|p| p.to_path_buf())
}

/// 安排 macOS 自更新：写一个分离脚本（等本体退出 → 挂载 dmg → ditto 替换 .app → 重启），
/// 启动它，然后延迟 ~1.5s 退出本体（留时间给前端渲染「即将重启」提示）。
#[cfg(target_os = "macos")]
fn macos_self_update(
    app: &AppHandle,
    dmg: &std::path::Path,
    bundle: &std::path::Path,
) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join("promptcast-update");
    std::fs::create_dir_all(&dir).map_err(|e| UpdateError::Io(e.to_string()))?;
    let script_path = dir.join("pc-selfupdate.sh");
    std::fs::write(&script_path, MACOS_UPDATE_SCRIPT).map_err(|e| UpdateError::Io(e.to_string()))?;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| UpdateError::Io(e.to_string()))?;

    // 写重启标记：脚本重启后，新实例在启动时检测到它就唤起抽屉，告知用户「已更新并重启」。
    let _ = std::fs::write(relaunch_flag_path(), "1");

    // 分离子进程：父进程退出后它被 init 收养、继续运行；用本进程 pid 作为「等谁退出」的目标。
    std::process::Command::new("/bin/sh")
        .arg(&script_path)
        .arg(dmg)
        .arg(bundle)
        .arg(std::process::id().to_string())
        .spawn()
        .map_err(|e| UpdateError::Launch(e.to_string()))?;

    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        handle.exit(0);
    });
    Ok(())
}

/// 等本体退出 → 挂载 dmg → 安全替换 .app（先 ditto 到 .new，成功才换）→ 去隔离属性 → 重启。
/// 任何一步失败都尽量不破坏旧 .app（只在 ditto 成功后才删旧），最差情况是仍打开旧版本。
#[cfg(target_os = "macos")]
const MACOS_UPDATE_SCRIPT: &str = r#"#!/bin/sh
DMG="$1"; APP="$2"; PID="$3"
i=0
while kill -0 "$PID" 2>/dev/null; do sleep 0.3; i=$((i+1)); [ "$i" -gt 100 ] && break; done
MNT=$(mktemp -d /tmp/pcupd.XXXXXX) || exit 0
if /usr/bin/hdiutil attach "$DMG" -nobrowse -noautoopen -mountpoint "$MNT" >/dev/null 2>&1; then
  SRC=$(/bin/ls -d "$MNT"/*.app 2>/dev/null | head -n1)
  if [ -n "$SRC" ]; then
    rm -rf "${APP}.new"
    if /usr/bin/ditto "$SRC" "${APP}.new"; then
      /usr/bin/xattr -dr com.apple.quarantine "${APP}.new" 2>/dev/null || true
      rm -rf "$APP" && mv "${APP}.new" "$APP"
    fi
  fi
  /usr/bin/hdiutil detach "$MNT" >/dev/null 2>&1 || true
fi
rmdir "$MNT" 2>/dev/null || true
/usr/bin/open "$APP"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_basic_semver() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn newer_tolerates_v_prefix() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.2.0", "v0.1.0"));
    }

    #[test]
    fn newer_prerelease_is_lower_than_release() {
        // semver: 0.2.0-beta < 0.2.0
        assert!(is_newer("0.2.0", "0.2.0-beta"));
        assert!(!is_newer("0.2.0-beta", "0.2.0"));
    }

    #[test]
    fn newer_falls_back_to_triple_when_unparsable() {
        // 非法 semver（四段）走三元组回退：1.2.3.4 -> (1,2,3)
        assert!(!is_newer("1.2.3.4", "1.2.3"));
        assert!(is_newer("1.3.0.0", "1.2.9"));
    }

    #[test]
    fn newer_garbage_is_not_update() {
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(!is_newer("", "0.1.0"));
    }

    #[test]
    fn platform_key_shape() {
        let k = platform_key();
        assert!(k.contains('-'), "key has os-arch shape: {k}");
        #[cfg(target_os = "windows")]
        assert!(k.starts_with("windows-"));
        #[cfg(target_os = "macos")]
        assert!(k.starts_with("darwin-"));
    }

    #[test]
    fn filename_strips_path_traversal() {
        assert_eq!(
            filename_from_url("https://x.cool/a/b/PromptCast_0.2.0_x64-setup.exe"),
            "PromptCast_0.2.0_x64-setup.exe"
        );
        // 查询串/奇怪字符被过滤，不产生子目录。
        let f = filename_from_url("https://x.cool/d/../../etc/passwd");
        assert!(!f.contains('/'));
    }

    #[test]
    fn url_scheme_policy() {
        assert!(is_secure_or_local("https://cnb.cool/x/update.json"));
        assert!(is_secure_or_local("http://localhost:8080/u.json"));
        assert!(is_secure_or_local("http://127.0.0.1:3000/u.json"));
        assert!(!is_secure_or_local("http://evil.example/u.json")); // 明文公网
        assert!(!is_secure_or_local("ftp://x/u.json"));
        // host 边界：前缀像 localhost 但其实是别的域名，必须拒。
        assert!(!is_secure_or_local("http://localhost.evil.com/u.json"));
        assert!(!is_secure_or_local("http://127.0.0.1.evil.com/u.json"));
        assert!(!is_secure_or_local("not a url"));
    }

    #[test]
    fn sha256_validity() {
        let good = "a".repeat(64);
        assert!(is_valid_sha256(&good));
        assert!(!is_valid_sha256("")); // 空串
        assert!(!is_valid_sha256(&"a".repeat(63))); // 太短
        assert!(!is_valid_sha256(&"g".repeat(64))); // 非 hex
        assert!(!is_valid_sha256("在此填安装包的sha256")); // 占位文字
    }

    #[test]
    fn hex_lower_formats_bytes() {
        assert_eq!(hex_lower(&[0x00, 0x0f, 0xab, 0xff]), "000fabff");
    }

    #[test]
    fn manifest_parses_with_optional_fields() {
        let j = r#"{
            "version": "0.2.0",
            "title": "新版",
            "notes": "改进若干",
            "downloads": {
                "windows-x86_64": { "url": "https://x/setup.exe", "sha256": "ABCD" },
                "darwin-aarch64": { "url": "https://x/app.dmg" }
            }
        }"#;
        let m: Manifest = serde_json::from_str(j).unwrap();
        assert_eq!(m.version, "0.2.0");
        assert_eq!(m.pub_date, None);
        assert_eq!(
            m.downloads["windows-x86_64"].sha256.as_deref(),
            Some("ABCD")
        );
        assert!(m.downloads["darwin-aarch64"].sha256.is_none());
    }
}
