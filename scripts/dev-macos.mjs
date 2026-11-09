// 用 Launch Services 启动独立 .app，让 TCC 权限归属于开发应用而不是终端宿主。
import { spawn, spawnSync } from "node:child_process";
import { mkdir, copyFile, writeFile, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

if (process.platform !== "darwin") throw new Error("dev:mac 仅用于 macOS；Windows 请运行 pnpm tauri dev。");
const root = fileURLToPath(new URL("../", import.meta.url));
const tauriDir = join(root, "src-tauri");
const config = JSON.parse(await readFile(join(tauriDir, "tauri.conf.json"), "utf8"));
const app = join(tauriDir, "target", "debug", "PromptCast Dev.app");
const executable = join(app, "Contents", "MacOS", "prompt-manager");
let server;
let launcher;

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} 执行失败（${result.status}）`);
}

function stop() {
  // 仅停止这个脚本创建的开发应用，避免留下旧进程继续使用旧的授权身份。
  const result = spawnSync("/bin/ps", ["-axo", "pid=,comm="], { encoding: "utf8" });
  for (const line of (result.stdout ?? "").split("\n")) {
    const match = line.trim().match(/^(\d+)\s+(.+)$/);
    if (match?.[2] === executable) {
      try { process.kill(Number(match[1]), "SIGTERM"); }
      catch (e) { if (e.code !== "ESRCH") console.error(e); }
    }
  }
  launcher?.kill("SIGTERM");
  server?.kill("SIGTERM");
}

process.once("SIGINT", () => { stop(); process.exit(0); });
process.once("SIGTERM", () => { stop(); process.exit(0); });

try {
  run("cargo", ["build", "--manifest-path", join(tauriDir, "Cargo.toml"), "--no-default-features"]);
  await mkdir(dirname(executable), { recursive: true });
  await mkdir(join(app, "Contents", "Resources"), { recursive: true });
  await copyFile(join(tauriDir, "target", "debug", "prompt-manager"), executable);
  await copyFile(join(tauriDir, "icons", "icon.icns"), join(app, "Contents", "Resources", "icon.icns"));
  const escape = (s) => String(s).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
  await writeFile(join(app, "Contents", "Info.plist"), `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>${escape(config.identifier)}.dev</string>
<key>CFBundleName</key><string>PromptCast Dev</string>
<key>CFBundleDisplayName</key><string>PromptCast Dev</string>
<key>CFBundleExecutable</key><string>prompt-manager</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>${escape(config.version)}</string>
<key>CFBundleShortVersionString</key><string>${escape(config.version)}</string>
<key>CFBundleIconFile</key><string>icon.icns</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSUIElement</key><true/>
</dict></plist>
`);
  run("/usr/bin/codesign", ["--force", "--sign", "-", app]);
  server = spawn("pnpm", ["dev"], { cwd: root, stdio: "inherit" });
  let serverError;
  server.on("error", (e) => { serverError = e; });
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (serverError) throw serverError;
    if (server.exitCode !== null) throw new Error("前端服务未能启动，请检查端口 1420 是否被占用。");
    try {
      const response = await fetch(config.build.devUrl, { signal: AbortSignal.timeout(500) });
      if (response.ok) { ready = true; break; }
    } catch { /* 等待 Vite 开始监听，最多等待约 30 秒。 */ }
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  if (!ready) throw new Error("等待前端服务超时。");
  console.log(`启动独立开发应用：${app}`);
  launcher = spawn("/usr/bin/open", ["-n", "-W", app], { stdio: "inherit" });
  await new Promise((resolve, reject) => {
    launcher.once("error", reject);
    launcher.once("exit", (code) => code === 0 ? resolve() : reject(new Error(`应用启动失败（${code}）`)));
  });
} finally {
  stop();
}
