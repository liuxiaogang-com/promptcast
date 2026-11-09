<script lang="ts">
// UpdateModal — 发现新版本时的弹窗：标题 / 说明 / 下载进度 / 拉起安装器。
// 由 useUpdateStore 驱动；在 DrawerView（启动静默查）与 SettingsView（手动查）都挂一份。
import { defineComponent } from "vue";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useUpdateStore } from "../stores/update";
import { windowSetModalOpen } from "../api/window";
import ReleaseNotes from "./ReleaseNotes.vue";
import { isMac } from "../utils/format";

export default defineComponent({
  name: "UpdateModal",
  components: { ReleaseNotes },
  data() {
    return { modalActive: false };
  },
  computed: {
    store() {
      return useUpdateStore();
    },
    open(): boolean {
      return this.store.visible && !!this.store.info;
    },
    launchedMsg(): string {
      // macOS：自动替换并重启（无需手动拖拽）；Windows：跑安装器。文案如实区分。
      return isMac()
        ? "更新包已下载，应用将自动关闭并完成更新，随后重新打开，请稍候…"
        : "安装程序已启动，请按提示完成安装。";
    },
  },
  watch: {
    // 弹窗出现时让抽屉「失焦不自动隐藏」，避免点别处把弹窗一起带走；关闭时撤销。
    // 用独立的 modal-open 标志，不动用户的 pin 状态（见后端 window_set_modal_open）。
    open(v: boolean) {
      this.setModalOpen(v);
    },
  },
  beforeUnmount() {
    this.setModalOpen(false);
  },
  methods: {
    isDrawer(): boolean {
      try {
        return getCurrentWebviewWindow().label === "drawer";
      } catch {
        return false;
      }
    },
    setModalOpen(open: boolean): void {
      // 仅抽屉有失焦自动隐藏；设置窗不需要。modalActive 去重，避免重复下发。
      if (!this.isDrawer()) return;
      if (open === this.modalActive) return;
      this.modalActive = open;
      void windowSetModalOpen(open).catch(() => {});
    },
    mb(bytes: number): string {
      return (bytes / 1024 / 1024).toFixed(1);
    },
    install(): void {
      void this.store.startInstall();
    },
    later(): void {
      // 下载中不允许点背景/关闭，避免中断下载、丢掉进度监听。
      if (this.store.downloading) return;
      this.store.dismiss();
    },
    skip(): void {
      if (this.store.downloading) return;
      this.store.skipVersion();
    },
    ignoreToday(): void {
      if (this.store.downloading) return;
      this.store.ignoreToday();
    },
  },
});
</script>

<template>
  <div v-if="open" class="upd-overlay" @click.self="later">
    <div class="upd-card" role="dialog" aria-modal="true">
      <div class="upd-head">
        <span class="upd-badge">新版本</span>
        <span class="upd-ver">v{{ store.info!.version }}</span>
        <span class="upd-cur">当前 v{{ store.info!.current }}</span>
      </div>

      <h3 v-if="store.info!.title" class="upd-title">{{ store.info!.title }}</h3>
      <div v-if="store.info!.pub_date" class="upd-date">{{ store.info!.pub_date }}</div>
      <ReleaseNotes :version="store.info!.version" />

      <div v-if="store.error" class="upd-err">{{ store.error }}</div>

      <div v-if="store.downloading || store.launched" class="upd-prog">
        <div class="bar">
          <div
            class="fill"
            :class="{ indet: !store.progress.total && !store.launched }"
            :style="store.launched
              ? { width: '100%' }
              : store.progress.total
                ? { width: store.percent + '%' }
                : {}"
          />
        </div>
        <div class="ptext">
          <template v-if="store.launched">
            {{ launchedMsg }}
          </template>
          <template v-else-if="store.progress.total">
            正在下载… {{ store.percent }}% ({{ mb(store.progress.downloaded) }} /
            {{ mb(store.progress.total) }} MB)
          </template>
          <template v-else>
            正在下载… {{ mb(store.progress.downloaded) }} MB
          </template>
        </div>
      </div>

      <div class="upd-actions">
        <template v-if="store.launched">
          <button class="ghost" @click="later">关闭</button>
        </template>
        <template v-else>
          <button class="ghost" :disabled="store.downloading" @click="skip">
            跳过当前版本
          </button>
          <button class="ghost" :disabled="store.downloading" @click="ignoreToday">
            今天忽略
          </button>
          <button class="primary" :disabled="store.downloading" @click="install">
            {{ store.downloading ? "下载中…" : "立即更新" }}
          </button>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.upd-overlay {
  position: fixed;
  inset: 0;
  z-index: 3000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.45);
  padding: 20px;
}
.upd-card {
  width: 100%;
  max-width: 380px;
  max-height: 80vh;
  overflow-y: auto;
  background: var(--bg-surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: var(--shadow-md);
  padding: 18px 18px 14px;
}
.upd-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.upd-badge {
  font-size: 11px;
  font-weight: 600;
  color: var(--accent-fg);
  background: var(--accent);
  border-radius: 5px;
  padding: 2px 7px;
}
.upd-ver {
  font-size: 16px;
  font-weight: 700;
  color: var(--text-primary);
}
.upd-cur {
  margin-left: auto;
  font-size: 12px;
  color: var(--text-tertiary);
}
.upd-title {
  margin: 12px 0 2px;
  font-size: 14px;
  color: var(--text-primary);
}
.upd-date {
  font-size: 12px;
  color: var(--text-tertiary);
}
.upd-err {
  margin-top: 10px;
  font-size: 12.5px;
  color: var(--danger);
}
.upd-prog {
  margin-top: 14px;
}
.bar {
  height: 6px;
  border-radius: 3px;
  background: var(--bg-base);
  overflow: hidden;
}
.fill {
  height: 100%;
  background: var(--accent);
  transition: width var(--dur-base) var(--ease-out);
}
.fill.indet {
  width: 35%;
  animation: upd-indet 1.1s ease-in-out infinite;
}
@keyframes upd-indet {
  0% { margin-left: -35%; }
  100% { margin-left: 100%; }
}
.ptext {
  margin-top: 6px;
  font-size: 12px;
  color: var(--text-secondary);
}
.upd-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
}
.upd-actions button {
  height: 30px;
  padding: 0 14px;
  border-radius: 7px;
  font-size: 13px;
  cursor: pointer;
  border: 1px solid var(--border);
}
.upd-actions button:disabled {
  opacity: 0.55;
  cursor: default;
}
.upd-actions .ghost {
  background: var(--bg-surface);
  color: var(--text-secondary);
}
.upd-actions .ghost:hover:not(:disabled) {
  background: var(--bg-hover);
}
.upd-actions .primary {
  background: var(--accent);
  color: var(--accent-fg);
  border-color: var(--accent);
}
.upd-actions .primary:hover:not(:disabled) {
  filter: brightness(1.05);
}
</style>
