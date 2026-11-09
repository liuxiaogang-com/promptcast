<script lang="ts">
import { defineComponent } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { ShieldCheck, ArrowRight, CheckCircle2, ShieldAlert, LoaderCircle, CircleAlert } from "lucide-vue-next";
import appIcon from "../../src-tauri/icons/128x128.png";
import { ensureBackendReady } from "../api";
import {
  permissionsAppTarget, permissionsCheckAccessibility,
  permissionsRequestAccessibility, permissionsDragApp,
  type PermissionAppTarget,
} from "../api/inject";
import { useSettingsStore } from "../stores/settings";
import { applyPersistedTheme } from "../composables/useTheme";
import { isMac } from "../utils/format";

export default defineComponent({
  name: "PermissionsView",
  components: { ShieldCheck, ArrowRight, CheckCircle2, ShieldAlert, LoaderCircle, CircleAlert },
  data() {
    return {
      appIcon,
      isMacOS: isMac(),
      target: null as PermissionAppTarget | null,
      granted: null as boolean | null,
      checkError: "",
      manualChecking: false,
      error: "",
      checking: false,
      timer: null as ReturnType<typeof setInterval> | null,
    };
  },
  computed: {
    status(): "checking" | "granted" | "denied" | "error" {
      if (this.checking && (this.manualChecking || this.granted === null)) return "checking";
      if (this.checkError) return "error";
      if (this.granted === null) return "checking";
      return this.granted ? "granted" : "denied";
    },
  },
  async mounted() {
    if (!this.isMacOS) return;
    // 检测独立于主题、应用路径等辅助数据，任一加载失败都不能中断轮询。
    window.addEventListener("focus", this.onFocus);
    this.timer = setInterval(() => { void this.checkIfVisible(); }, 1500);
    await this.recheck();
    try {
      await ensureBackendReady();
      this.target = await permissionsAppTarget();
    } catch (e) { this.error = String(e); }
    try {
      const settings = useSettingsStore();
      await settings.loadAll();
      applyPersistedTheme(settings.data.theme);
    } catch (e) { this.error = String(e); }
  },
  beforeUnmount() {
    window.removeEventListener("focus", this.onFocus);
    if (this.timer) clearInterval(this.timer);
  },
  methods: {
    onFocus() { void this.recheck(); },
    async checkIfVisible() {
      try {
        if (await getCurrentWindow().isVisible()) await this.recheck();
      } catch (e) { this.checkError = String(e); }
    },
    async recheck(manual = false) {
      if (manual) this.manualChecking = true;
      if (this.checking) return;
      this.checking = true;
      try {
        this.granted = await permissionsCheckAccessibility();
        this.checkError = "";
      } catch (e) {
        this.granted = null;
        this.checkError = String(e);
      } finally {
        this.checking = false;
        this.manualChecking = false;
      }
    },
    async openSettings() {
      this.error = "";
      try {
        await permissionsRequestAccessibility();
        await this.recheck(true);
      }
      catch (e) { this.error = String(e); }
    },
    async startDrag() {
      if (!this.target) return;
      this.error = "";
      try { await permissionsDragApp(); }
      catch (e) { this.error = String(e); }
    },
    async showInFinder() {
      if (!this.target) return;
      this.error = "";
      try { await revealItemInDir(this.target.path); }
      catch (e) { this.error = String(e); }
    },
    async close() {
      try { await getCurrentWindow().close(); }
      catch (e) { this.error = String(e); }
    },
  },
});
</script>

<template>
  <main class="permission-guide">
    <ShieldCheck :size="30" :class="['shield', { authorized: status === 'granted' }]" />
    <h1>{{ status === 'granted' ? "辅助功能已授权" : "允许 PromptCast 自动粘贴" }}</h1>
    <p class="intro">授权后，可以把选中的内容直接粘贴到其他应用。暂不授权仍可使用复制功能。</p>

    <template v-if="isMacOS">
      <section :class="['permission-status', status]" role="status" aria-live="polite" :aria-busy="checking">
        <LoaderCircle v-if="status === 'checking'" :size="24" class="spin" />
        <CheckCircle2 v-else-if="status === 'granted'" :size="24" />
        <CircleAlert v-else-if="status === 'error'" :size="24" />
        <ShieldAlert v-else :size="24" />
        <div>
          <strong v-if="status === 'checking'">正在检测辅助功能权限…</strong>
          <strong v-else-if="status === 'granted'">已授权 · 自动粘贴可用</strong>
          <strong v-else-if="status === 'error'">检测失败 · 暂时无法确认权限</strong>
          <strong v-else>未授权 · 当前仅可复制</strong>
          <p v-if="status === 'granted'">系统已确认当前运行的 PromptCast 获得辅助功能权限。</p>
          <p v-else-if="status === 'error'">{{ checkError }}</p>
          <p v-else-if="status === 'denied'">系统仍未允许当前进程自动粘贴。若开关已开启，请完全退出再重新打开应用后检测。</p>
          <p v-else>正在向系统查询当前进程的实际权限。</p>
        </div>
      </section>
      <div class="check-feedback">
        <button class="text-button" :disabled="checking" @click="recheck(true)">{{ checking && manualChecking ? "检测中…" : "重新检测" }}</button>
      </div>
      <p v-if="status === 'granted'" class="hint">可以关闭此窗口并开始使用。撤销权限后，本窗口也会自动更新状态。</p>
      <template v-else>
        <ol>
          <li>打开系统设置的「隐私与安全 → 辅助功能」。</li>
          <li v-if="target?.bundled">将下方图标拖入右侧应用列表，再开启对应开关。</li>
        </ol>
        <button class="primary open-settings" @click="openSettings">打开辅助功能设置 <ArrowRight :size="16" /></button>
        <div v-if="target?.bundled" class="drag-card">
          <button class="app-file" :disabled="!target" aria-label="拖拽应用到辅助功能列表；按回车在访达中显示"
            @mousedown.left.prevent="startDrag" @keydown.enter.prevent="showInFinder" @keydown.space.prevent="showInFinder">
            <img :src="appIcon" alt="PromptCast" :draggable="false" />
            <strong>{{ target?.bundled ? "PromptCast.app" : "prompt-manager" }}</strong>
          </button>
          <span>按住图标，拖到系统设置的应用列表</span>
          <button class="text-button" :disabled="!target" @click="showInFinder">找不到或拖不进去？在访达中显示</button>
        </div>
        <p v-if="target && !target.bundled" class="dev-note">当前通过命令行启动，系统可能将权限归属给启动工具，授权此文件不会生效。请退出后使用 pnpm dev:mac 启动独立开发应用，再授权该应用。</p>
        <p class="hint">添加到列表后还需手动开启开关。本窗口会自动检测；仍未生效时请重启 PromptCast。</p>
      </template>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <footer>
        <button :class="status === 'granted' ? 'primary' : 'secondary'" @click="close">{{ status === 'granted' ? "完成" : "稍后再说" }}</button>
      </footer>
    </template>
    <p v-else>当前平台无需 macOS 辅助功能授权。</p>
  </main>
</template>

<style scoped>
.permission-guide { height: 100vh; overflow-y: auto; box-sizing: border-box; padding: 28px; background: var(--bg-surface); color: var(--text-primary); }
.shield { color: var(--text-secondary); }
h1 { font-size: var(--fs-21); margin: 12px 0; font-weight: 600; }
.intro, ol, .hint, .dev-note { font-size: var(--fs-13); line-height: 1.7; color: var(--text-secondary); }
ol { padding-left: 20px; margin: 16px 0; }
li + li { margin-top: 6px; }
button { border-radius: 8px; padding: 9px 14px; cursor: pointer; font-size: var(--fs-13); }
button:disabled { opacity: .5; cursor: default; }
button:focus-visible { outline: 2px solid var(--text-primary); outline-offset: 3px; }
.primary { background: var(--accent); color: var(--accent-fg); border: 1px solid var(--accent); }
.secondary { background: var(--bg-surface); color: var(--text-primary); border: 1px solid var(--border-strong); }
.open-settings { display: flex; align-items: center; justify-content: center; gap: 8px; width: 100%; }
.drag-card { margin-top: 18px; border: 1px dashed var(--border-strong); border-radius: 12px; background: var(--bg-base); padding: 16px; display: flex; flex-direction: column; align-items: center; gap: 8px; font-size: var(--fs-12); color: var(--text-secondary); }
.app-file { display: flex; flex-direction: column; align-items: center; gap: 8px; background: transparent; border: none; color: var(--text-primary); cursor: grab; user-select: none; }
.app-file:active { cursor: grabbing; }
.app-file img { width: 64px; height: 64px; pointer-events: none; }
.text-button { border: none; background: transparent; color: var(--text-secondary); text-decoration: underline; padding: 4px; font-size: var(--fs-12); }
.dev-note { padding: 10px; border-radius: 8px; background: var(--bg-hover); margin-top: 12px; font-size: var(--fs-12); }
.hint { font-size: var(--fs-12); margin: 12px 0; }
 .shield.authorized { color: var(--success); }
.permission-status { display: flex; align-items: flex-start; gap: 12px; padding: 16px; margin-top: 16px; border: 1px solid currentColor; border-radius: 12px; }
.permission-status > svg { flex-shrink: 0; }
.permission-status strong { font-size: var(--fs-14); }
.permission-status p { font-size: var(--fs-12); line-height: 1.6; margin: 6px 0 0; overflow-wrap: anywhere; }
.permission-status.denied { color: var(--warning); background: color-mix(in srgb, var(--warning) 9%, var(--bg-surface)); }
.permission-status.granted { color: var(--success); background: color-mix(in srgb, var(--success) 10%, var(--bg-surface)); }
.permission-status.error { color: var(--danger); background: color-mix(in srgb, var(--danger) 8%, var(--bg-surface)); }
.permission-status.checking { color: var(--text-secondary); background: var(--bg-hover); }
.check-feedback { display: flex; align-items: center; justify-content: flex-end; margin-top: 8px; }
.spin { animation: spin 1s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
.error { color: var(--danger); font-size: var(--fs-12); overflow-wrap: anywhere; }
footer { display: flex; justify-content: flex-end; gap: 10px; margin-top: 20px; }
</style>
