<script lang="ts">
// UpdateModal — 发现新版本时的弹窗：标题 / 说明 / 下载进度 / 拉起安装器。
// 只挂载于独立更新窗口，下载与确认安装分离。
import { defineComponent } from "vue";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useUpdateStore } from "../stores/update";
import ReleaseNotes from "./ReleaseNotes.vue";
export default defineComponent({
  components: { ReleaseNotes },
  computed: {
    store() { return useUpdateStore(); },
    open(): boolean { return !!this.store.info; },
    busy(): boolean { return this.store.downloading || this.store.installing || this.store.launched; },
    date(): string {
      const value = this.store.info?.pub_date;
      if (!value) return "";
      const date = new Date(value);
      return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
    },
  },
  methods: {
    mb(bytes: number): string { return (bytes / 1024 / 1024).toFixed(1); },
    install() { void (this.store.ready ? this.store.startInstall() : this.store.startDownload()); },
    later() { if (!this.store.installing) void getCurrentWebviewWindow().close(); },
    skip() { if (!this.busy) { this.store.skipVersion(); this.later(); } },
    ignoreToday() { if (!this.busy) { this.store.ignoreToday(); this.later(); } },
  },
});
</script>

<template>
  <div v-if="open" class="upd-overlay">
    <div class="upd-card" role="dialog" aria-modal="true">
      <div class="upd-head">
        <span class="upd-badge">新版本</span>
        <span class="upd-ver">v{{ store.info!.version }}</span>
        <span class="upd-cur">当前 v{{ store.info!.current }}</span>
      </div>

      <h3 v-if="store.info!.title" class="upd-title">{{ store.info!.title }}</h3>
      <div v-if="store.info!.pub_date" class="upd-date">{{ date }}</div>
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
            正在退出应用并安装，请稍候…
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

      <p v-if="store.ready && !store.launched" class="ptext">下载完成。请先保存正在编辑的内容，点击“退出并安装”后才会关闭应用。</p>
      <div class="upd-actions">
        <button class="ghost" :disabled="store.installing || store.launched" @click="later">稍后再说</button>
        <button v-if="!store.ready" class="ghost" :disabled="busy" @click="skip">跳过当前版本</button>
        <button v-if="!store.ready" class="ghost" :disabled="busy" @click="ignoreToday">今天忽略</button>
        <button class="primary" :disabled="busy" @click="install">{{ store.installing || store.launched ? "正在安装…" : store.downloading ? "下载中…" : store.ready ? "退出并安装" : "下载更新" }}</button>
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
  background: var(--bg-surface);
  padding: 20px;
}
.upd-card {
  width: 100%;
  max-width: 100%;
  max-height: 100%;
  overflow-y: auto;
  background: var(--bg-surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: none;
  padding: 18px 18px 14px;
}
.upd-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.upd-badge {
  font-size: var(--fs-11);
  font-weight: 600;
  color: var(--accent-fg);
  background: var(--accent);
  border-radius: 5px;
  padding: 2px 7px;
}
.upd-ver {
  font-size: var(--fs-16);
  font-weight: 700;
  color: var(--text-primary);
}
.upd-cur {
  margin-left: auto;
  font-size: var(--fs-12);
  color: var(--text-tertiary);
}
.upd-title {
  margin: 12px 0 2px;
  font-size: var(--fs-14);
  color: var(--text-primary);
}
.upd-date {
  font-size: var(--fs-12);
  color: var(--text-tertiary);
}
.upd-err {
  margin-top: 10px;
  font-size: var(--fs-12-5);
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
  font-size: var(--fs-12);
  color: var(--text-secondary);
}
.upd-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
}
.upd-actions button {
  min-height: calc(30px * var(--font-scale));
  padding: 0 14px;
  border-radius: 7px;
  font-size: var(--fs-13);
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
