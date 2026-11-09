// stores/update.ts — 应用更新状态机。
// 启动静默查（drawer）/ 手动查（设置）→ 有新版弹 UpdateModal → 下载（监听 update-progress）
// → 后端拉起安装器。下载是阻塞后端命令，进度靠事件推。
import { defineStore } from "pinia";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  updateCheck,
  updateDownloadInstall,
  updateGetManifestUrl,
  updateSetManifestUrl,
} from "../api/update";
import { EVT_UPDATE_PROGRESS } from "../composables/useAppEvents";
import type { UpdateInfo, UpdateProgress } from "../types/update";

// 抑制状态持久化在 localStorage（同源跨窗口共享，存活于应用重启）。
const LS_SKIP = "pc.update.skipVersion"; // 用户「跳过当前版本」记住的版本号
const LS_IGNORE = "pc.update.ignoreDate"; // 用户「今天忽略」的日期（YYYY-MM-DD）
const LS_AUTO = "pc.update.autoCheck"; // 自动检查更新开关（"0"=关，其余=开）

function lsGet(k: string): string {
  try { return localStorage.getItem(k) ?? ""; } catch { return ""; }
}
function lsSet(k: string, v: string): void {
  try { localStorage.setItem(k, v); } catch { /* 隐私模式等忽略 */ }
}
/** 本地日期 YYYY-MM-DD。 */
function todayStr(): string {
  const d = new Date();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${m}-${day}`;
}

interface State {
  info: UpdateInfo | null;
  visible: boolean;
  checking: boolean;
  downloading: boolean;
  launched: boolean;
  progress: UpdateProgress;
  error: string | null;
  manifestUrl: string;
  skippedVersion: string; // 被「跳过当前版本」抑制的版本
  ignoredDate: string; // 被「今天忽略」抑制的日期
  autoCheckEnabled: boolean; // 自动检查更新开关（仅用于设置页 UI 绑定）
  unlisten: UnlistenFn | null;
}

export const useUpdateStore = defineStore("update", {
  state: (): State => ({
    info: null,
    visible: false,
    checking: false,
    downloading: false,
    launched: false,
    progress: { downloaded: 0, total: null },
    error: null,
    manifestUrl: "",
    skippedVersion: lsGet(LS_SKIP),
    ignoredDate: lsGet(LS_IGNORE),
    autoCheckEnabled: lsGet(LS_AUTO) !== "0", // 默认开
    unlisten: null,
  }),
  getters: {
    percent(s): number {
      if (!s.progress.total || s.progress.total <= 0) return 0;
      return Math.min(
        100,
        Math.round((s.progress.downloaded / s.progress.total) * 100),
      );
    },
  },
  actions: {
    async loadManifestUrl(): Promise<void> {
      try {
        this.manifestUrl = await updateGetManifestUrl();
      } catch {
        /* ignore */
      }
    },
    async setManifestUrl(url: string): Promise<void> {
      await updateSetManifestUrl(url);
      this.manifestUrl = url.trim();
    },
    /**
     * 查更新。manual=false 为启动静默查（找不到/无新版都不打扰）；
     * manual=true 返回布尔让调用方决定提示（“已是最新” / “未配置”）。
     * 返回 true = 有新版且已弹窗。
     */
    async check(manual = false): Promise<boolean> {
      if (this.checking) return false;
      // 自动查受开关控制；直接读 localStorage（而非内存态），使设置窗里改的开关对抽屉窗即时生效。
      if (!manual && lsGet(LS_AUTO) === "0") return false;
      this.checking = true;
      this.error = null;
      try {
        const info = await updateCheck();
        if (info) {
          this.info = info;
          // 自动（静默）查会被「跳过当前版本 / 今天忽略」抑制，避免反复打扰；
          // 手动查（设置页「检查更新」）一律照常弹，让用户随时能更新被跳过的版本。
          const suppressed =
            !manual &&
            (info.version === this.skippedVersion ||
              this.ignoredDate === todayStr());
          if (suppressed) return false;
          // 复位上一轮下载残留，否则二次弹窗会带着 launched=true / 旧进度条出现。
          this.launched = false;
          this.progress = { downloaded: 0, total: null };
          this.visible = true;
          return true;
        }
        return false;
      } catch (e) {
        // 静默查不打扰用户，只有手动查才暴露错误。
        if (manual) this.error = String(e);
        return false;
      } finally {
        this.checking = false;
      }
    },
    async startInstall(): Promise<void> {
      if (this.downloading || !this.info) return;
      this.downloading = true;
      this.launched = false;
      this.error = null;
      this.progress = { downloaded: 0, total: null };
      // 监听进度（一次性绑定，下载结束/出错后解绑）。
      if (!this.unlisten) {
        this.unlisten = await listen<UpdateProgress>(
          EVT_UPDATE_PROGRESS,
          (e) => {
            this.progress = e.payload;
          },
        );
      }
      try {
        await updateDownloadInstall(this.info.version);
        // 命令返回 = 安装器已拉起（Windows setup/msi、macOS dmg 已打开）。
        this.launched = true;
      } catch (e) {
        this.error = String(e);
      } finally {
        this.downloading = false;
        if (this.unlisten) {
          this.unlisten();
          this.unlisten = null;
        }
      }
    },
    /** 开/关自动检查更新。关闭后启动与每 30 分钟的静默查都不再执行；手动「检查更新」不受影响。 */
    setAutoCheckEnabled(on: boolean): void {
      this.autoCheckEnabled = on;
      lsSet(LS_AUTO, on ? "1" : "0");
    },
    /** 跳过当前版本：记住该版本号，以后自动查到同一版本不再弹（手动查仍会弹）。 */
    skipVersion(): void {
      if (this.info) {
        this.skippedVersion = this.info.version;
        lsSet(LS_SKIP, this.skippedVersion);
      }
      this.dismiss();
    },
    /** 今天忽略：记住今天，今日内自动查不再弹；次日自动恢复。 */
    ignoreToday(): void {
      this.ignoredDate = todayStr();
      lsSet(LS_IGNORE, this.ignoredDate);
      this.dismiss();
    },
    dismiss(): void {
      this.visible = false;
      this.error = null;
      this.launched = false;
      this.progress = { downloaded: 0, total: null };
      if (this.unlisten) {
        this.unlisten();
        this.unlisten = null;
      }
    },
  },
});
