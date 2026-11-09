<!--
  SettingsView.vue — 设置窗口（独立）。
-->
<script lang="ts">
import { defineComponent } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  Sliders, Keyboard, Palette, FolderTree, Globe, Database,
  ShieldCheck, Info, FileText,
  Cloud, RefreshCw, LogOut,
} from "lucide-vue-next";

import HotkeyRecorder from "../components/settings/HotkeyRecorder.vue";
import FoldersPanel from "../components/settings/FoldersPanel.vue";
import SitesPanel from "../components/settings/SitesPanel.vue";
import DataPanel from "../components/settings/DataPanel.vue";
import BaseToast from "../components/ui/BaseToast.vue";
import UpdateModal from "../components/UpdateModal.vue";

import { FONT_SIZE_OPTIONS, applyFontScale } from "../composables/useDisplay";
import { useSettingsStore } from "../stores/settings";
import {
  registerHotkey,
  unregisterHotkey,
  windowOpenPermissions,
  windowOpenChangelog,
} from "../api/window";
import {
  permissionsCheckAccessibility,
} from "../api/inject";
import { logDir, ensureBackendReady } from "../api";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { setThemeMode, applyPersistedTheme } from "../composables/useTheme";
import {
  listenAppEvent,
  EVT_THEME_CHANGED, EVT_SETTINGS_CHANGED,
  EVT_FOLDERS_CHANGED, EVT_TAGS_CHANGED, EVT_SITES_CHANGED,
  EVT_SYNC_STATUS_CHANGED,
} from "../composables/useAppEvents";
import { useFoldersStore } from "../stores/folders";
import { useTagsStore } from "../stores/tags";
import { useSitesStore } from "../stores/sites";
import { useAuthStore } from "../stores/auth";
import { useSyncStore } from "../stores/sync";
import { useUpdateStore } from "../stores/update";
import { useUIStore } from "../stores/ui";
import type { SyncStatus } from "../types/sync";
import { isMac, relativeTime } from "../utils/format";
import { confirmDanger } from "../utils/dialog";
import { log } from "../utils/logger";
import { getVersion } from "@tauri-apps/api/app";
import type { ThemeMode } from "../types/settings";

type TabKey =
  | "general" | "account" | "hotkey" | "theme" | "folders"
  | "sites" | "data" | "permissions" | "about";

interface NavItem {
  key: TabKey;
  label: string;
  icon: any;
}

export default defineComponent({
  name: "SettingsView",
  components: {
    HotkeyRecorder, FoldersPanel, SitesPanel, DataPanel, BaseToast, UpdateModal,
    Sliders, Keyboard, Palette, FolderTree, Globe, Database,
    ShieldCheck, Info, FileText,
    Cloud, RefreshCw, LogOut,
  },
  data() {
    return {
      tab: "general" as TabKey,
      hotkeyDraft: "",
      hotkeyBusy: false,
      displayBusy: false,
      fontSizeOptions: FONT_SIZE_OPTIONS,
      accessibilityOk: true,
      isMacOS: isMac(),
      logsPath: "",
      appVer: "",
      // 更新（清单地址已预制在后端，不再可配置）
      updBusy: false,
      // 账户 / 同步
      emailDraft: "",
      passwordDraft: "",
      serverDraft: "",
      authBusy: false,
      authErr: "",
      // 账户管理
      pwOld: "",
      pwNew: "",
      pwMsg: "",
      delPassword: "",
      showDelete: false,
      acctBusy: false,
      // 找回密码
      forgotMode: false,
      forgotEmail: "",
      resetToken: "",
      resetNew: "",
      resetMsg: "",
      unlisteners: [] as UnlistenFn[],
    };
  },
  computed: {
    settings() { return useSettingsStore(); },
    auth() { return useAuthStore(); },
    sync() { return useSyncStore(); },
    updateStore() { return useUpdateStore(); },
    uiStore() { return useUIStore(); },
    appVersion(): string { return this.appVer || "…"; },
    lastSyncText(): string {
      const t = this.sync.status.last_sync_at;
      return t ? relativeTime(t) : "尚未同步";
    },
    navItems(): NavItem[] {
      return [
        { key: "general", label: "常规", icon: Sliders },
        { key: "account", label: "账户同步", icon: Cloud },
        { key: "hotkey", label: "快捷键", icon: Keyboard },
        { key: "theme", label: "显示", icon: Palette },
        { key: "folders", label: "分类管理", icon: FolderTree },
        { key: "sites", label: "网址快捷", icon: Globe },
        { key: "data", label: "数据", icon: Database },
        { key: "permissions", label: "权限诊断", icon: ShieldCheck },
        { key: "about", label: "关于", icon: Info },
      ];
    },
  },
  async mounted() {
    log.info("SettingsView mounted");
    await ensureBackendReady();
    if (!this.settings.loaded) await this.settings.loadAll();
    applyPersistedTheme(this.settings.data.theme);
    this.hotkeyDraft = this.settings.data.hotkey ?? "";
    this.accessibilityOk = await permissionsCheckAccessibility();
    window.addEventListener("focus", this.recheckAccess);
    try { this.logsPath = await logDir(); } catch { /* */ }
    try { this.appVer = await getVersion(); } catch { /* */ }

    try { await this.auth.load(); } catch { /* */ }
    try { await this.sync.load(); this.serverDraft = this.sync.serverUrl; } catch { /* */ }

    this.unlisteners.push(
      await listenAppEvent<ThemeMode>(EVT_THEME_CHANGED, (m) => applyPersistedTheme(m)),
      await listenAppEvent(EVT_SETTINGS_CHANGED, () => this.settings.loadAll()),
      await listenAppEvent(EVT_FOLDERS_CHANGED, () => useFoldersStore().loadAll()),
      await listenAppEvent(EVT_TAGS_CHANGED, () => useTagsStore().loadAll()),
      await listenAppEvent(EVT_SITES_CHANGED, () => useSitesStore().loadAll()),
      await listenAppEvent<SyncStatus>(EVT_SYNC_STATUS_CHANGED, (s) => {
        this.sync.apply(s);
        this.auth.loggedIn = s.logged_in;
        this.auth.email = s.email;
      }),
    );
  },
  beforeUnmount() {
    window.removeEventListener("focus", this.recheckAccess);
    for (const u of this.unlisteners) u();
  },
  methods: {
    setTab(t: TabKey) { this.tab = t; },
    async setTheme(mode: "system" | "light" | "dark") {
      log.info(`theme switched to ${mode}`);
      await this.settings.set("theme", mode);
      setThemeMode(mode);
    },
    async setFontScale(value: number) {
      if (this.displayBusy) return;
      this.displayBusy = true;
      try {
        await this.settings.set("font_scale", value);
        applyFontScale(value);
      } catch (e) {
        log.error(`font scale save failed: ${e}`);
        this.uiStore.pushToast("字号保存失败，请重试", "danger");
      } finally {
        this.displayBusy = false;
      }
    },
    async setDefaultAction(v: "inject" | "copy_only") {
      await this.settings.set("default_action", v);
    },
    async toggleAutoStart(v: boolean) {
      await this.settings.set("auto_start", v);
    },
    async setClipboardEnabled(v: boolean) {
      await this.settings.set("clipboard_history_enabled", v);
    },
    async setClipboardLimit(n: number) {
      await this.settings.set("clipboard_history_limit", n);
    },
    async saveHotkey(value: string) {
      if (this.hotkeyBusy) return;
      this.hotkeyBusy = true;
      this.hotkeyDraft = value;
      log.info(`saving hotkey: ${this.hotkeyDraft}`);
      try {
        if (this.hotkeyDraft) await registerHotkey(this.hotkeyDraft);
        else await unregisterHotkey();
        await this.settings.loadAll();
        this.uiStore.pushToast(value ? "快捷键已保存" : "快捷键已清除", "success");
      } catch (e) {
        log.error(`register hotkey failed: ${e}`);
        this.uiStore.pushToast(String(e), "danger");
        await this.settings.loadAll();
      } finally {
        this.hotkeyDraft = this.settings.data.hotkey ?? "";
        this.hotkeyBusy = false;
      }
    },
    async recheckAccess() {
      this.accessibilityOk = await permissionsCheckAccessibility();
    },
    async requestAccess() {
      await windowOpenPermissions();
    },
    async openLogsDir() {
      if (!this.logsPath) return;
      try {
        await revealItemInDir(this.logsPath);
      } catch (e) {
        log.error(`open logs dir failed: ${e}`);
      }
    },
    async openChangelog() {
      try {
        await windowOpenChangelog();
      } catch (e) {
        log.error(`open changelog failed: ${e}`);
        this.uiStore.pushToast("无法打开版本日志窗口，请重试", "danger");
      }
    },
    // ---- 更新 ----
    // 清单地址已预制在后端（不可配置）。手动查一律弹窗（即使该版本被「跳过」过）。
    async checkUpdates() {
      if (this.updBusy) return;
      this.updBusy = true;
      try {
        const found = await this.updateStore.check(true);
        if (!found) {
          if (this.updateStore.error) {
            this.uiStore.pushToast(`检查更新失败: ${this.updateStore.error}`, "danger");
          } else {
            this.uiStore.pushToast(`已是最新版本（v${this.appVersion}）`, "success");
          }
        }
      } finally {
        this.updBusy = false;
      }
    },
    // ---- 账户 / 同步 ----
    async saveServer() {
      try { await this.sync.setServerUrl(this.serverDraft.trim()); }
      catch (e) { this.authErr = `保存服务器地址失败: ${e}`; }
    },
    async doLogin() {
      await this.authAction(() => this.auth.login(this.emailDraft.trim(), this.passwordDraft));
    },
    async doRegister() {
      await this.authAction(() => this.auth.register(this.emailDraft.trim(), this.passwordDraft));
    },
    async authAction(fn: () => Promise<void>) {
      this.authErr = "";
      this.authBusy = true;
      try {
        await this.saveServer();
        await fn();
        this.passwordDraft = "";
        await this.sync.load();
      } catch (e) {
        this.authErr = String(e);
      } finally {
        this.authBusy = false;
      }
    },
    async doLogout() {
      try { await this.auth.logout(); await this.sync.load(); }
      catch (e) { this.authErr = String(e); }
    },
    async toggleSync(on: boolean) {
      try { await this.sync.setEnabled(on); await this.sync.load(); }
      catch (e) { this.authErr = String(e); }
    },
    async doSyncNow() {
      try { await this.sync.now(); } catch (e) { this.authErr = String(e); }
    },
    async doChangePassword() {
      this.pwMsg = "";
      this.acctBusy = true;
      try {
        await this.auth.changePassword(this.pwOld, this.pwNew);
        this.pwOld = "";
        this.pwNew = "";
        this.pwMsg = "密码已修改";
      } catch (e) { this.pwMsg = String(e); }
      finally { this.acctBusy = false; }
    },
    async doDeleteAccount() {
      if (!(await confirmDanger("确认删除账户？不可撤销，本机已同步的数据也会被清除。", "删除账户"))) return;
      this.authErr = "";
      this.acctBusy = true;
      try {
        await this.auth.deleteAccount(this.delPassword);
        this.delPassword = "";
        this.showDelete = false;
        await this.sync.load();
      } catch (e) { this.authErr = String(e); }
      finally { this.acctBusy = false; }
    },
    async doForgot() {
      this.resetMsg = "";
      this.acctBusy = true;
      try {
        const email = (this.forgotEmail || this.emailDraft).trim();
        const t = await this.auth.forgotPassword(email);
        if (t) {
          this.resetToken = t;
          this.resetMsg = "已生成重置令牌（本地开发回显，已自动填好下方令牌）";
        } else {
          this.resetMsg = "若该邮箱已注册，重置令牌已发送到邮箱";
        }
      } catch (e) { this.resetMsg = String(e); }
      finally { this.acctBusy = false; }
    },
    async doReset() {
      this.resetMsg = "";
      this.acctBusy = true;
      try {
        await this.auth.resetPassword(this.resetToken.trim(), this.resetNew);
        this.resetMsg = "密码已重置，请用新密码登录";
        this.resetToken = "";
        this.resetNew = "";
        this.forgotMode = false;
      } catch (e) { this.resetMsg = String(e); }
      finally { this.acctBusy = false; }
    },
  },
});
</script>

<template>
  <div class="settings">
    <div class="body">
      <nav class="nav">
        <button
          v-for="n in navItems"
          :key="n.key"
          :class="{ on: tab === n.key }"
          @click="setTab(n.key)"
        >
          <component :is="n.icon" :size="14" />
          <span>{{ n.label }}</span>
        </button>
      </nav>
      <main class="main">
        <!-- 常规 -->
        <section v-if="tab === 'general'" class="panel">
          <h3>常规</h3>
          <div class="card">
            <div class="row">
              <div>
                <div class="title">默认动作</div>
                <div class="sub">在抽屉里按 Enter 时执行的操作。</div>
              </div>
            </div>
            <label class="radio-card" :class="{ on: settings.data.default_action === 'inject' }">
              <input type="radio" :checked="settings.data.default_action === 'inject'"
                @change="setDefaultAction('inject')" />
              <div>
                <div class="title">自动注入到上一个聚焦窗口</div>
                <div class="sub">隐藏抽屉 → 模拟粘贴；失败回退仅复制。</div>
              </div>
            </label>
            <label class="radio-card" :class="{ on: settings.data.default_action === 'copy_only' }">
              <input type="radio" :checked="settings.data.default_action === 'copy_only'"
                @change="setDefaultAction('copy_only')" />
              <div>
                <div class="title">仅复制到剪贴板</div>
                <div class="sub">不需要辅助功能权限；用户自行 {{ isMacOS ? "⌘V" : "Ctrl+V" }}。</div>
              </div>
            </label>
          </div>
          <div class="card">
            <div class="row">
              <div>
                <div class="title">开机自启</div>
                <div class="sub">登录时自动启动并在后台监听快捷键。</div>
              </div>
              <span class="spacer" />
              <label class="switch">
                <input type="checkbox" :checked="settings.data.auto_start"
                  @change="(e) => toggleAutoStart((e.target as HTMLInputElement).checked)" />
                <span />
              </label>
            </div>
          </div>
          <div class="card">
            <div class="row">
              <div>
                <div class="title">记录剪贴板历史</div>
                <div class="sub">后台记录复制的内容，在抽屉「剪贴板」分类里查看。支持文本和图片；图片仅保存在本机。</div>
              </div>
              <span class="spacer" />
              <label class="switch">
                <input type="checkbox" :checked="settings.data.clipboard_history_enabled"
                  @change="(e) => setClipboardEnabled((e.target as HTMLInputElement).checked)" />
                <span />
              </label>
            </div>
            <div v-if="settings.data.clipboard_history_enabled" class="row">
              <div>
                <div class="title">保留条数</div>
                <div class="sub">超出后自动丢弃最旧的记录。</div>
              </div>
              <span class="spacer" />
              <select
                :value="settings.data.clipboard_history_limit"
                @change="(e) => setClipboardLimit(Number((e.target as HTMLSelectElement).value))"
                style="min-height: calc(30px * var(--font-scale));border-radius:6px;border:1px solid var(--border);background:var(--bg-surface);color:var(--text-primary);font-size: var(--fs-12);padding:0 8px;"
              >
                <option :value="100">100</option>
                <option :value="200">200</option>
                <option :value="500">500</option>
                <option :value="1000">1000</option>
                <option :value="2000">2000</option>
              </select>
            </div>
          </div>
        </section>

        <!-- 账户 / 同步 -->
        <section v-if="tab === 'account'" class="panel">
          <h3>账户与多设备同步</h3>
          <p class="lead">
            登录后，提示词 / 文件夹 / 标签 / 网站会在你的多台设备间自动同步（离线优先，联网即同步）。
            设置、剪贴板历史保持本地。
          </p>

          <!-- 未登录 -->
          <div v-if="!auth.loggedIn" class="card">
            <label class="field">
              <span class="flabel">服务器地址</span>
              <input v-model="serverDraft" class="inp" placeholder="http://localhost:3000" />
            </label>
            <label class="field">
              <span class="flabel">邮箱</span>
              <input v-model="emailDraft" class="inp" type="email" placeholder="you@example.com"
                autocomplete="username" />
            </label>
            <label class="field">
              <span class="flabel">密码</span>
              <input v-model="passwordDraft" class="inp" type="password" placeholder="至少 8 位"
                autocomplete="current-password" @keydown.enter="doLogin" />
            </label>
            <div v-if="authErr" class="err">{{ authErr }}</div>
            <div class="row">
              <button class="linklike" @click="forgotMode = !forgotMode">
                {{ forgotMode ? "返回登录" : "忘记密码？" }}
              </button>
              <span class="spacer" />
              <button class="ghost" :disabled="authBusy" @click="doRegister">注册</button>
              <button class="primary" :disabled="authBusy" @click="doLogin">
                {{ authBusy ? "请稍候…" : "登录" }}
              </button>
            </div>
            <template v-if="forgotMode">
              <div class="divider" />
              <label class="field">
                <span class="flabel">注册邮箱</span>
                <div class="row">
                  <input v-model="forgotEmail" class="inp" type="email"
                    :placeholder="emailDraft || 'you@example.com'" />
                  <button class="ghost" :disabled="acctBusy" @click="doForgot">获取令牌</button>
                </div>
              </label>
              <label class="field">
                <span class="flabel">重置令牌</span>
                <input v-model="resetToken" class="inp" placeholder="粘贴邮件/本地回显的令牌" />
              </label>
              <label class="field">
                <span class="flabel">新密码</span>
                <input v-model="resetNew" class="inp" type="password" placeholder="至少 8 位" />
              </label>
              <div v-if="resetMsg" class="hint">{{ resetMsg }}</div>
              <div class="row">
                <span class="spacer" />
                <button class="primary" :disabled="acctBusy || !resetToken || resetNew.length < 8"
                  @click="doReset">重置密码</button>
              </div>
            </template>
          </div>

          <!-- 已登录 -->
          <template v-else>
            <div v-if="authErr" class="err">{{ authErr }}</div>
            <div class="card">
              <div class="row">
                <div>
                  <div class="title">{{ auth.email || "已登录" }}</div>
                  <div class="sub">
                    状态：{{ sync.status.enabled ? sync.status.state : "已暂停" }} · 上次同步：{{ lastSyncText }}
                    <template v-if="sync.status.pending > 0 && sync.status.enabled"> · 待推送 {{ sync.status.pending }}</template>
                  </div>
                  <div v-if="sync.status.message" class="sub" style="color:var(--danger)">
                    {{ sync.status.message }}
                  </div>
                </div>
                <span class="spacer" />
                <button class="ghost danger" @click="doLogout">
                  <LogOut :size="13" /> 登出
                </button>
              </div>
            </div>
            <div class="card">
              <div class="row">
                <div>
                  <div class="title">启用同步</div>
                  <div class="sub">关闭后本地照常用，只是不再上传/下载。</div>
                </div>
                <span class="spacer" />
                <button class="ghost" @click="doSyncNow">
                  <RefreshCw :size="13" /> 立即同步
                </button>
                <label class="switch">
                  <input type="checkbox" :checked="sync.status.enabled"
                    @change="(e) => toggleSync((e.target as HTMLInputElement).checked)" />
                  <span />
                </label>
              </div>
              <label class="field">
                <span class="flabel">服务器地址</span>
                <div class="row">
                  <input v-model="serverDraft" class="inp" placeholder="http://localhost:3000" />
                  <button class="ghost" @click="saveServer">保存</button>
                </div>
              </label>
            </div>

            <div class="card">
              <div class="title">修改密码</div>
              <label class="field">
                <span class="flabel">当前密码</span>
                <input v-model="pwOld" class="inp" type="password" autocomplete="current-password" />
              </label>
              <label class="field">
                <span class="flabel">新密码</span>
                <input v-model="pwNew" class="inp" type="password" placeholder="至少 8 位"
                  autocomplete="new-password" />
              </label>
              <div v-if="pwMsg" class="hint">{{ pwMsg }}</div>
              <div class="row">
                <span class="spacer" />
                <button class="primary" :disabled="acctBusy || !pwOld || pwNew.length < 8"
                  @click="doChangePassword">修改密码</button>
              </div>
            </div>

            <div class="card">
              <div class="row">
                <div>
                  <div class="title" style="color:var(--danger)">删除账户</div>
                  <div class="sub">永久删除账户及服务端数据，并清除本机已同步数据，不可撤销。</div>
                </div>
                <span class="spacer" />
                <button class="ghost danger" @click="showDelete = !showDelete">
                  {{ showDelete ? "取消" : "删除账户" }}
                </button>
              </div>
              <template v-if="showDelete">
                <label class="field">
                  <span class="flabel">输入当前密码确认</span>
                  <input v-model="delPassword" class="inp" type="password" />
                </label>
                <div class="row">
                  <span class="spacer" />
                  <button class="ghost danger" :disabled="acctBusy || !delPassword"
                    @click="doDeleteAccount">确认删除</button>
                </div>
              </template>
            </div>
          </template>
        </section>

        <!-- 快捷键 -->
        <section v-if="tab === 'hotkey'" class="panel">
          <h3>全局唤起快捷键</h3>
          <p class="lead">
            点击下方快捷键录制，按下组合键后自动保存，Esc 取消。建议使用 ⌘⇧P / Ctrl+Shift+P。
          </p>
          <div class="card">
            <div class="row">
              <div class="title">打开抽屉</div>
              <span class="spacer" />
              <HotkeyRecorder :model-value="hotkeyDraft" :disabled="hotkeyBusy" @update:model-value="saveHotkey" />
            </div>
          </div>
        </section>

        <!-- 主题 -->
        <section v-if="tab === 'theme'" class="panel">
          <h3>显示</h3>
          <div class="title">主题</div>
          <div class="theme-row">
            <button
              class="theme-btn"
              :class="{ on: settings.data.theme === 'system' }"
              @click="setTheme('system')"
            >
              <div class="card-prev sys">A·a</div>
              <span>跟随系统</span>
            </button>
            <button
              class="theme-btn"
              :class="{ on: settings.data.theme === 'light' }"
              @click="setTheme('light')"
            >
              <div class="card-prev light">A</div>
              <span>浅色</span>
            </button>
            <button
              class="theme-btn"
              :class="{ on: settings.data.theme === 'dark' }"
              @click="setTheme('dark')"
            >
              <div class="card-prev dark">A</div>
              <span>深色</span>
            </button>
          </div>
          <div class="title">字号</div>
          <div class="theme-row">
            <button v-for="option in fontSizeOptions" :key="option.value" class="theme-btn"
              :class="{ on: settings.data.font_scale === option.value }"
              :aria-pressed="settings.data.font_scale === option.value" :disabled="displayBusy"
              @click="setFontScale(option.value)">
              <span>{{ option.label }}</span>
            </button>
          </div>
          <p class="lead">立即应用到所有窗口，重启后保留。</p>
        </section>

        <FoldersPanel v-if="tab === 'folders'" />
        <SitesPanel v-if="tab === 'sites'" />
        <DataPanel v-if="tab === 'data'" />

        <!-- 权限诊断 -->
        <section v-if="tab === 'permissions'" class="panel permissions-panel">
          <h3>权限诊断</h3>
          <div class="card">
            <div class="row diagnostic-row">
              <div class="diagnostic-copy">
                <div class="title">{{ isMacOS ? "macOS 辅助功能" : "键盘模拟权限" }}</div>
                <div class="sub">
                  {{
                    isMacOS
                      ? "注入功能需要此权限（AXIsProcessTrusted）。未授权时按 Enter 会回退仅复制。"
                      : "Windows 上注入通常无需额外授权；若目标窗口为管理员权限运行，本程序需以同等权限启动方可注入。"
                  }}
                </div>
              </div>
              <span :class="['status', accessibilityOk ? 'ok' : 'bad']">
                {{ accessibilityOk ? "已授权" : "未授权" }}
              </span>
            </div>
            <div v-if="!accessibilityOk && isMacOS" class="hint">
              点下面按钮打开授权引导，将
              <b>PromptCast</b>（开发模式下可能显示为 prompt-manager）拖入系统「辅助功能」列表并开启开关；返回本窗口会自动重新检测。
            </div>
            <div class="row">
              <span class="spacer" />
              <button class="ghost" @click="recheckAccess">重新检测</button>
              <button v-if="!accessibilityOk && isMacOS" class="primary" @click="requestAccess">
                请求授权
              </button>
            </div>
          </div>
          <div class="card">
            <div class="row diagnostic-row">
              <div class="diagnostic-copy">
                <div class="title">日志</div>
                <div class="sub">所有操作和错误会写入此目录的 app.log。出问题时把这个文件给开发者。</div>
              </div>
              <button class="ghost" @click="openLogsDir">
                <FileText :size="13" /> 打开日志目录
              </button>
            </div>
            <div class="path">{{ logsPath || "(加载中)" }}</div>
          </div>
        </section>

        <!-- 关于 -->
        <section v-if="tab === 'about'" class="panel">
          <h3>关于</h3>
          <div class="card">
            <div class="row">
              <div>
                <div class="title">PromptCast</div>
                <div class="sub">v{{ appVersion }} · MIT License</div>
              </div>
              <span class="spacer" />
              <button class="ghost" @click="openChangelog">
                <FileText :size="13" /> 版本日志
              </button>
              <button class="primary" :disabled="updBusy" @click="checkUpdates">
                <RefreshCw :size="13" /> {{ updBusy ? "检查中…" : "检查更新" }}
              </button>
            </div>
          </div>
          <div class="card">
            <div class="row">
              <div>
                <div class="title">自动检查更新</div>
                <div class="sub">
                  启动时及每 30 分钟静默检查；发现新版本才弹窗，可选「跳过当前版本/今天忽略」。
                  关闭后仅在你手动点「检查更新」时才查。
                </div>
              </div>
              <span class="spacer" />
              <label class="switch">
                <input type="checkbox" :checked="updateStore.autoCheckEnabled"
                  @change="(e) => updateStore.setAutoCheckEnabled((e.target as HTMLInputElement).checked)" />
                <span />
              </label>
            </div>
          </div>
        </section>
      </main>
    </div>
    <BaseToast />
    <UpdateModal />
  </div>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--bg-surface);
  color: var(--text-primary);
}
.body { flex: 1; display: flex; min-height: 0; }
.nav {
  width: calc(160px * var(--font-scale));
  flex-shrink: 0;
  overflow-y: auto;
  padding: 12px 8px;
  background: var(--bg-base);
  border-right: 1px solid var(--border);
  display: flex; flex-direction: column; gap: 2px;
}
.nav button {
  min-height: calc(30px * var(--font-scale)); padding: 0 10px;
  border-radius: 6px;
  display: flex; align-items: center; gap: 8px;
  text-align: left;
  font-size: var(--fs-12);
  color: var(--text-secondary);
}
.nav button:hover { background: var(--bg-hover); }
.nav button.on {
  background: var(--bg-selected);
  color: var(--text-primary);
  font-weight: 600;
  box-shadow: inset 2.5px 0 0 var(--accent);
}

.main { flex: 1; min-width: 0; padding: 24px; overflow-y: auto; }

.panel { display: flex; flex-direction: column; gap: 12px; }
.panel h3 { font-size: var(--fs-14); font-weight: 600; }
.lead { font-size: var(--fs-12); color: var(--text-secondary); line-height: 1.5; }

.card {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px;
  background: var(--bg-base);
  display: flex; flex-direction: column; gap: 10px;
}
.row { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.row > div { min-width: 0; overflow-wrap: anywhere; }
.row button, .row .status, .switch { flex-shrink: 0; }
.title { font-size: var(--fs-13); font-weight: 500; }
.sub { font-size: var(--fs-11); color: var(--text-secondary); margin-top: 2px; line-height: 1.5; }
.spacer { flex: 1; }
.path {
  font-family: var(--font-mono);
  font-size: var(--fs-10-5);
  color: var(--text-tertiary);
  background: var(--bg-input);
  padding: 6px 8px;
  border-radius: 6px;
  word-break: break-all;
}

.permissions-panel .diagnostic-row { flex-wrap: wrap; }
.permissions-panel .diagnostic-copy { flex: 1 1 200px; min-width: 0; overflow-wrap: anywhere; }
.permissions-panel .status,
.permissions-panel button { flex-shrink: 0; white-space: nowrap; }
.permissions-panel button svg { flex-shrink: 0; }
.permissions-panel .path { word-break: normal; overflow-wrap: anywhere; }

.radio-card {
  display: flex; align-items: center; gap: 10px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  cursor: pointer;
}
.radio-card.on { border-color: var(--accent); border-width: 1.5px; }
.radio-card input { accent-color: var(--accent); }

.switch {
  position: relative;
  width: 34px; height: 20px;
  display: inline-block;
}
.switch input { position: absolute; opacity: 0; pointer-events: none; }
.switch span {
  position: absolute; inset: 0;
  background: var(--border-strong);
  border-radius: 999px;
  cursor: pointer;
  transition: background var(--dur-fast);
}
.switch span::after {
  content: "";
  position: absolute;
  width: 16px; height: 16px;
  background: #ffffff;
  border-radius: 50%;
  top: 2px; left: 2px;
  transition: left var(--dur-fast) var(--ease-out);
  box-shadow: 0 1px 2px rgba(0,0,0,0.2);
}
.switch input:checked + span { background: var(--accent); }
.switch input:checked + span::after { left: 16px; }

.theme-row { display: flex; flex-wrap: wrap; gap: 12px; }
.theme-btn {
  width: calc(120px * var(--font-scale));
  max-width: 100%;
  padding: 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-base);
  display: flex; flex-direction: column; gap: 8px; align-items: center;
  font-size: var(--fs-11);
  color: var(--text-secondary);
}
.theme-btn.on { border-color: var(--accent); border-width: 1.5px; }
.card-prev {
  width: 80px; min-height: calc(50px * var(--font-scale));
  border-radius: 6px;
  display: flex; align-items: center; justify-content: center;
  font-weight: 700;
}
.card-prev.light { background: #ffffff; color: #18181b; border: 1px solid #ececee; }
.card-prev.dark { background: #17171a; color: #fafafa; border: 1px solid #2a2a2f; }
.card-prev.sys {
  background: linear-gradient(135deg, #ffffff 50%, #17171a 50%);
  color: #888;
  border: 1px solid var(--border);
}

.ghost {
  min-height: calc(30px * var(--font-scale)); padding: 0 12px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  font-size: var(--fs-12);
  white-space: nowrap;
  color: var(--text-secondary);
  display: inline-flex; align-items: center; gap: 6px;
}
.ghost:hover { background: var(--bg-hover); color: var(--text-primary); }
.primary {
  min-height: calc(30px * var(--font-scale)); padding: 0 14px;
  border-radius: 6px;
  background: var(--accent);
  color: var(--accent-fg);
  font-size: var(--fs-12);
  font-weight: 600;
}

.status { font-size: var(--fs-11); padding: 2px 8px; border-radius: 4px; font-weight: 500; }
.status.ok { background: var(--accent-soft); color: var(--success); }
.status.bad { background: var(--accent-soft); color: var(--danger); }

.field { display: flex; flex-direction: column; gap: 6px; }
.flabel { font-size: var(--fs-11); font-weight: 600; color: var(--text-secondary); }
.inp {
  flex: 1;
  min-height: calc(32px * var(--font-scale)); padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg-input);
  color: var(--text-primary);
  font-size: var(--fs-13);
  outline: none;
}
.inp:focus { border-color: var(--accent); }
.err {
  font-size: var(--fs-11);
  color: var(--danger);
  background: var(--accent-soft);
  padding: 6px 10px;
  border-radius: 6px;
  word-break: break-all;
}
.ghost.danger { color: var(--danger); border-color: transparent; }
.linklike {
  background: none;
  color: var(--accent);
  font-size: var(--fs-11);
  padding: 0;
}
.linklike:hover { text-decoration: underline; }
.divider { height: 1px; background: var(--border); margin: 4px 0; }
.hint {
  font-size: var(--fs-11);
  padding: 8px 10px;
  background: var(--bg-hover);
  border-radius: 6px;
  color: var(--text-secondary);
}
</style>
