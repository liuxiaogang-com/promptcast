<!--
  DrawerView.vue — 主抽屉窗口（400×720）。
  组合 SearchBar / FilterChips / PromptList / SiteLauncher / HintBar。
-->
<script lang="ts">
import { defineComponent } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";

import SearchBar from "../components/drawer/SearchBar.vue";
import FilterChips from "../components/drawer/FilterChips.vue";
import PromptList from "../components/drawer/PromptList.vue";
import ClipboardList from "../components/drawer/ClipboardList.vue";
import SiteLauncher from "../components/drawer/SiteLauncher.vue";
import HintBar from "../components/drawer/HintBar.vue";
import BaseToast from "../components/ui/BaseToast.vue";
import UpdateModal from "../components/UpdateModal.vue";

import { usePromptsStore } from "../stores/prompts";
import { useFoldersStore } from "../stores/folders";
import { useTagsStore } from "../stores/tags";
import { useSitesStore } from "../stores/sites";
import { useClipboardStore } from "../stores/clipboard";
import { useSettingsStore } from "../stores/settings";
import { useSyncStore } from "../stores/sync";
import { useUpdateStore } from "../stores/update";
import { useUIStore } from "../stores/ui";

import { confirmDanger } from "../utils/dialog";
import { buildSearchable, searchPrompts } from "../composables/useFuzzySearch";
import { applyPersistedTheme } from "../composables/useTheme";
import {
  listenAppEvent,
  EVT_PROMPTS_CHANGED,
  EVT_FOLDERS_CHANGED,
  EVT_TAGS_CHANGED,
  EVT_SITES_CHANGED,
  EVT_SETTINGS_CHANGED,
  EVT_THEME_CHANGED,
  EVT_CLIPBOARD_CHANGED,
  EVT_SYNC_STATUS_CHANGED,
} from "../composables/useAppEvents";
import {
  injectPaste,
  injectCopyOnly,
} from "../api/inject";
import {
  windowOpenEditor,
  windowOpenPreview,
  windowHideDrawer,
  windowOpenSettings,
} from "../api/window";
import { ensureBackendReady } from "../api";
import { log } from "../utils/logger";

import type { Prompt } from "../types/prompt";
import { contentCopy, clipboardSave } from "../api/media";
import type { ClipEntry } from "../types/clipboard";
import type { ThemeMode } from "../types/settings";
import type { SyncStatus } from "../types/sync";

export default defineComponent({
  name: "DrawerView",
  components: {
    SearchBar, FilterChips, PromptList, ClipboardList, SiteLauncher, HintBar, BaseToast, UpdateModal,
  },
  data() {
    return {
      query: "",
      clipSelectedId: null as number | null,
      ctxMenu: { visible: false, x: 0, y: 0, id: null as number | null },
      unlisteners: [] as UnlistenFn[],
      updTimer: null as ReturnType<typeof setInterval> | null,
    };
  },
  computed: {
    prompts() { return usePromptsStore(); },
    settings() { return useSettingsStore(); },
    ui() { return useUIStore(); },
    folders() { return useFoldersStore(); },
    tags() { return useTagsStore(); },
    clip() { return useClipboardStore(); },
    syncStore() { return useSyncStore(); },
    updateStore() { return useUpdateStore(); },
    syncBadge(): { show: boolean; cls: string; label: string } | null {
      const s = this.syncStore.status;
      if (!s.logged_in || !s.enabled) return null;
      if (s.state === "syncing") return { show: true, cls: "syncing", label: "同步中…" };
      if (s.state === "error" || s.state === "offline")
        return { show: true, cls: "bad", label: s.state === "offline" ? "离线" : "同步出错" };
      if (s.pending > 0) return { show: true, cls: "pending", label: `待同步 ${s.pending}` };
      return null;
    },
    isClip(): boolean { return this.ui.activeChipKey === "clipboard"; },
    clipFiltered(): ClipEntry[] {
      const list = this.clip.list;
      const q = this.query.trim().toLowerCase();
      if (!q) return list;
      return list.filter((c) => c.content.toLowerCase().includes(q));
    },
    chipFiltered(): Prompt[] {
      const list = this.prompts.list;
      const k = this.ui.activeChipKey;
      if (k === "all") return list;
      if (k === "favorites") return list.filter((p) => p.is_favorite);
      if (k.startsWith("folder:")) {
        const id = Number(k.slice(7));
        return list.filter((p) => p.folder_id === id);
      }
      if (k.startsWith("tag:")) {
        const id = Number(k.slice(4));
        return list.filter((p) => p.tag_ids.includes(id));
      }
      return list;
    },
    searched(): Prompt[] {
      if (!this.query.trim()) return this.chipFiltered;
      const { items } = buildSearchable(this.chipFiltered, this.tags.list);
      return searchPrompts(items, this.query).map((it) => it.raw);
    },
    counts(): Record<string, number> {
      const list = this.prompts.list;
      const out: Record<string, number> = {
        all: list.length,
        favorites: list.filter((p) => p.is_favorite).length,
        clipboard: this.clip.list.length,
      };
      for (const f of this.folders.list) {
        out[`folder:${f.id}`] = list.filter((p) => p.folder_id === f.id).length;
      }
      for (const t of this.tags.list) {
        out[`tag:${t.id}`] = list.filter((p) => p.tag_ids.includes(t.id)).length;
      }
      return out;
    },
    selectedPrompt(): Prompt | null {
      const id = this.prompts.selectedId;
      if (id == null) return null;
      return this.searched.find((p) => p.id === id)
        ?? this.prompts.list.find((p) => p.id === id)
        ?? null;
    },
    ctxTarget(): Prompt | null {
      if (this.ctxMenu.id == null) return null;
      return this.prompts.list.find((p) => p.id === this.ctxMenu.id) ?? null;
    },
  },
  watch: {
    searched(list: Prompt[]) {
      if (!list.find((p) => p.id === this.prompts.selectedId)) {
        this.prompts.select(list[0]?.id ?? null);
      }
    },
    clipFiltered(list: ClipEntry[]) {
      if (!list.find((c) => c.id === this.clipSelectedId)) {
        this.clipSelectedId = list[0]?.id ?? null;
      }
    },
    isClip(on: boolean) {
      // 切到剪贴板分类时拉一次最新历史，并选中第一条。
      if (on) {
        this.clip.loadAll(this.settings.data.clipboard_history_limit);
        this.clipSelectedId = this.clipFiltered[0]?.id ?? null;
      }
    },
  },
  async mounted() {
    log.info("DrawerView mounted");
    // 等后端 DbState 就绪再加载，否则会和 setup() 的 manage 抢跑、命令报
    // "state not managed for db"，mounted 中途抛错 → 卡「加载中」、事件监听也没注册上。
    await ensureBackendReady();
    // 先加载设置，恢复持久化的排序，再拉列表（否则总是回到「最近使用」）。
    await this.settings.loadAll();
    this.prompts.sortMode = this.settings.data.sort_mode;
    await Promise.all([
      this.folders.loadAll(),
      this.tags.loadAll(),
      this.sites().loadAll(),
      this.prompts.loadAll(),
      this.clip.loadAll(this.settings.data.clipboard_history_limit),
    ]);
    applyPersistedTheme(this.settings.data.theme);
    try { await this.syncStore.load(); } catch { /* 同步未配置/未登录 */ }
    document.addEventListener("keydown", this.onKey);
    await this.subscribeEvents();
    // 启动静默查更新（无新版/出错都不打扰）；有新版且未被「跳过/今天忽略」才弹 UpdateModal。
    void this.updateStore.check(false);
    // 之后每 30 分钟静默查一次。抽屉窗常驻（隐藏时 JS 定时器照跑），适合做轮询源。
    this.updTimer = setInterval(() => { void this.updateStore.check(false); }, 30 * 60 * 1000);
  },
  beforeUnmount() {
    document.removeEventListener("keydown", this.onKey);
    if (this.updTimer) { clearInterval(this.updTimer); this.updTimer = null; }
    for (const u of this.unlisteners) u();
  },
  methods: {
    sites() { return useSitesStore(); },
    openSettings() { void windowOpenSettings(); },
    async subscribeEvents() {
      this.unlisteners.push(
        await listenAppEvent(EVT_PROMPTS_CHANGED, () => this.prompts.loadAll()),
        await listenAppEvent(EVT_FOLDERS_CHANGED, () => this.folders.loadAll()),
        await listenAppEvent(EVT_TAGS_CHANGED, () => this.tags.loadAll()),
        await listenAppEvent(EVT_SITES_CHANGED, () => this.sites().loadAll()),
        await listenAppEvent(EVT_CLIPBOARD_CHANGED, () => this.clip.loadAll(this.settings.data.clipboard_history_limit)),
        await listenAppEvent(EVT_SETTINGS_CHANGED, () => this.settings.loadAll()),
        await listenAppEvent<ThemeMode>(EVT_THEME_CHANGED, (m) => applyPersistedTheme(m)),
        await listenAppEvent<SyncStatus>(EVT_SYNC_STATUS_CHANGED, (s) => this.syncStore.apply(s)),
      );
    },
    focusSearch() {
      const inp = document.querySelector<HTMLInputElement>(".search-row input");
      inp?.focus(); inp?.select();
    },
    moveSel(dir: 1 | -1) {
      if (this.isClip) {
        const list = this.clipFiltered;
        if (!list.length) return;
        const cur = list.findIndex((c) => c.id === this.clipSelectedId);
        const next = (cur + dir + list.length) % list.length;
        this.clipSelectedId = list[next].id;
        return;
      }
      const list = this.searched;
      if (!list.length) return;
      const cur = list.findIndex((p) => p.id === this.prompts.selectedId);
      const next = (cur + dir + list.length) % list.length;
      this.prompts.select(list[next].id);
    },
    // ---- 剪贴板模式 ----
    selectedClip(): ClipEntry | null {
      return this.clipFiltered.find((c) => c.id === this.clipSelectedId) ?? null;
    },
    async injectClipSelected() {
      const c = this.selectedClip();
      if (!c) return;
      try {
        const r = await contentCopy(c.id, "clipboard", true);
        log.info(`clip inject ok=${r.ok} fallback=${r.fallback}`);
        if (!r.ok) {
          this.ui.pushToast(r.message ?? "注入失败 · 已复制到剪贴板", "warning");
        }
      } catch (e) {
        this.ui.pushToast(`注入失败: ${e}`, "danger");
      }
    },
    onClipSelect(id: number) { this.clipSelectedId = id; },
    async injectClip(id: number) {
      this.clipSelectedId = id;
      await this.$nextTick();
      await this.injectClipSelected();
    },
    async saveClip(id: number, favorite = false) {
      try {
        const p = await clipboardSave(id, favorite);
        this.ui.pushToast(favorite ? "已保存并加入收藏" : "已保存到内容库", "success");
        if (!favorite) await windowOpenEditor(p.id);
      } catch (e) { this.ui.pushToast(`保存失败: ${e}`, "danger"); }
    },
    async deleteClip(id: number) {
      try { await this.clip.remove(id); }
      catch (e) { log.error(`deleteClip failed: ${e}`); }
    },
    // ---- 剪贴板批量操作（多选）----
    clipsContent(ids: number[]): string {
      if (this.clip.list.some((c) => ids.includes(c.id) && c.image_id)) {
        this.ui.pushToast("合并操作仅支持文本，请单独复制图片", "warning");
        return "";
      }
      const byId = new Map(this.clip.list.map((c) => [c.id, c.content]));
      return ids.map((id) => byId.get(id) ?? "").filter((s) => s.length > 0).join("\n");
    },
    async bulkPasteClips(ids: number[]) {
      const text = this.clipsContent(ids);
      if (!text) return;
      try {
        const r = await injectPaste(text);
        if (!r.ok) this.ui.pushToast(r.message ?? "注入失败 · 已复制到剪贴板", "warning");
      } catch (e) {
        this.ui.pushToast(`注入失败: ${e}`, "danger");
      }
    },
    async bulkCopyClips(ids: number[]) {
      const text = this.clipsContent(ids);
      if (!text) return;
      await injectCopyOnly(text);
      this.ui.pushToast(`已复制 ${ids.length} 条到剪贴板`, "success");
      await windowHideDrawer();
    },
    async bulkDeleteClips(ids: number[]) {
      if (!ids.length) return;
      if (!(await confirmDanger(`删除选中的 ${ids.length} 条剪贴板记录？`, "批量删除"))) return;
      try { await this.clip.removeMany(ids); }
      catch (e) { log.error(`bulkDeleteClips failed: ${e}`); }
    },
    async copyClipSelected() {
      const c = this.selectedClip();
      if (!c) return;
      await contentCopy(c.id, "clipboard");
      this.ui.pushToast("已复制到剪贴板", "success");
      await windowHideDrawer();
    },
    // 根据当前模式分发注入 / 复制。
    triggerInject() { return this.isClip ? this.injectClipSelected() : this.injectSelected(); },
    triggerCopy() { return this.isClip ? this.copyClipSelected() : this.copySelected(); },
    // ---- 列表右键菜单 ----
    onContext(p: { id: number; x: number; y: number }) {
      const MW = 150, MH = 156;
      const x = Math.min(p.x, window.innerWidth - MW - 8);
      const y = Math.min(p.y, window.innerHeight - MH - 8);
      this.ctxMenu = { visible: true, x, y, id: p.id };
    },
    closeCtx() { this.ctxMenu.visible = false; },
    async ctxCopy() {
      const p = this.ctxTarget; this.closeCtx();
      if (!p) return;
      await contentCopy(p.id, "prompt");
      this.ui.pushToast("已复制内容", "success");
    },
    async ctxDuplicate() {
      const p = this.ctxTarget; this.closeCtx();
      if (!p) return;
      await this.prompts.create({
        title: `${p.title} (副本)`,
        content: p.content,
        rich_content: p.rich_content,
        folder_id: p.folder_id,
        tag_ids: [...p.tag_ids],
      });
      this.ui.pushToast("已复制为新条目", "success");
    },
    async ctxTogglePin() {
      const id = this.ctxMenu.id; this.closeCtx();
      if (id != null) {
        try { await this.prompts.togglePin(id); }
        catch (e) { log.error(`togglePin failed: ${e}`); }
      }
    },
    async ctxDelete() {
      const p = this.ctxTarget; this.closeCtx();
      if (!p) return;
      if (!(await confirmDanger(`删除「${p.title}」？此操作不可撤销。`, "删除提示词"))) return;
      try { await this.prompts.remove(p.id); }
      catch (e) { log.error(`delete failed: ${e}`); }
    },
    async injectSelected() {
      log.info("injectSelected entry");
      const p = this.selectedPrompt;
      if (!p) {
        log.warn("injectSelected: nothing selected");
        return;
      }
      log.info(`injectSelected id=${p.id} title=${p.title}`);
      if (this.settings.data.default_action === "copy_only") {
        log.info("default_action=copy_only; falling through to copy");
        await this.copySelected();
        return;
      }
      try {
        const r = await contentCopy(p.id, "prompt", true);
        log.info(`injectPaste returned ok=${r.ok} fallback=${r.fallback} msg=${r.message}`);
        if (r.ok) {
          await this.prompts.recordUse(p.id);
        } else {
          this.ui.pushToast(r.message ?? "注入失败 · 已复制到剪贴板", "warning");
        }
      } catch (e) {
        log.error(`injectSelected exception: ${e}`);
        this.ui.pushToast(`注入失败: ${e}`, "danger");
      }
    },
    async copySelected() {
      const p = this.selectedPrompt;
      if (!p) return;
      log.info(`copy prompt id=${p.id}`);
      await contentCopy(p.id, "prompt");
      await this.prompts.recordUse(p.id);
      this.ui.pushToast("已复制到剪贴板", "success");
      await windowHideDrawer();
    },
    async editSelected() {
      const p = this.selectedPrompt;
      if (p) await windowOpenEditor(p.id);
    },
    async previewSelected() {
      const p = this.selectedPrompt;
      if (p) await windowOpenPreview(p.id);
    },
    async newPrompt() {
      log.info("new prompt window opening");
      await windowOpenEditor(null);
    },
    onSelect(id: number) {
      log.info(`[DrawerView] onSelect id=${id}`);
      this.prompts.select(id);
    },
    async toggleFav(id: number) {
      log.info(`toggleFav id=${id}`);
      try { await this.prompts.toggleFavorite(id); }
      catch (e) { log.error(`toggleFav failed: ${e}`); }
    },
    async injectById(id: number) {
      log.info(`injectById id=${id}`);
      this.prompts.select(id);
      await this.$nextTick();
      await this.injectSelected();
    },
    async editById(id: number) {
      log.info(`editById id=${id}`);
      try { await windowOpenEditor(id); }
      catch (e) { log.error(`editById failed: ${e}`); }
    },
    onKey(e: KeyboardEvent) {
      if (this.ctxMenu.visible) {
        if (e.key === "Escape") { e.preventDefault(); this.closeCtx(); }
        return; // 右键菜单打开时屏蔽列表快捷键
      }
      const cmd = e.metaKey || e.ctrlKey;
      const target = e.target as HTMLElement | null;
      const inEditable =
        target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA");
      if (e.key === "ArrowDown") { e.preventDefault(); this.moveSel(1); return; }
      if (e.key === "ArrowUp") { e.preventDefault(); this.moveSel(-1); return; }
      if (e.key === "Enter" && !inEditable) {
        e.preventDefault(); this.triggerInject(); return;
      }
      if (e.key === "Enter" && inEditable && target?.tagName === "INPUT") {
        e.preventDefault(); this.triggerInject(); return;
      }
      if (cmd && (e.key === "c" || e.key === "C") && !inEditable) {
        e.preventDefault(); this.triggerCopy(); return;
      }
      if (cmd && (e.key === "e" || e.key === "E") && !this.isClip) {
        e.preventDefault(); this.editSelected(); return;
      }
      if (cmd && (e.key === "n" || e.key === "N")) {
        e.preventDefault(); this.newPrompt(); return;
      }
      if (cmd && (e.key === "f" || e.key === "F")) {
        e.preventDefault(); this.focusSearch(); return;
      }
      if (e.key === " " && !inEditable && !this.isClip) {
        e.preventDefault(); this.previewSelected(); return;
      }
      if (e.key === "Escape") {
        if (!this.ui.drawerPinned) windowHideDrawer();
        return;
      }
    },
  },
});
</script>

<template>
  <div class="drawer">
    <SearchBar v-model="query" />
    <FilterChips :counts="counts" />
    <PromptList
      v-if="!isClip"
      :prompts="searched"
      :selected-id="prompts.selectedId"
      @select="onSelect"
      @toggle-fav="toggleFav"
      @inject="injectById"
      @edit="editById"
      @new-prompt="newPrompt"
      @context="onContext"
    />
    <ClipboardList
      v-else
      :entries="clipFiltered"
      :selected-id="clipSelectedId"
      @inject="injectClip"
      @delete="deleteClip"
      @bulk-delete="bulkDeleteClips"
      @save="saveClip($event)"
      @favorite="saveClip($event, true)"
      @bulk-paste="bulkPasteClips"
      @bulk-copy="bulkCopyClips"
    />
    <SiteLauncher />
    <HintBar :count="isClip ? clipFiltered.length : searched.length" />
    <div
      v-if="syncBadge"
      class="sync-strip"
      :class="syncBadge.cls"
      title="账户与同步设置"
      @click="openSettings"
    >
      {{ syncBadge.label }}
    </div>
    <BaseToast />
    <UpdateModal />

    <!-- 列表右键菜单 -->
    <div
      v-if="ctxMenu.visible"
      class="ctx-overlay"
      @click="closeCtx"
      @contextmenu.prevent="closeCtx"
    >
      <div class="ctx-menu" :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }" @click.stop>
        <button @click="ctxCopy">复制内容</button>
        <button @click="ctxDuplicate">复制为副本</button>
        <button @click="ctxTogglePin">{{ ctxTarget && ctxTarget.is_pinned ? "取消置顶" : "置顶" }}</button>
        <button class="danger" @click="ctxDelete">删除</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.drawer {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  background: var(--bg-base);
  color: var(--text-primary);
  border-radius: 12px;
  overflow: hidden;
  border: 1px solid var(--border);
}
.sync-strip {
  flex-shrink: 0;
  text-align: center;
  font-size: var(--fs-10-5);
  padding: 3px 8px;
  cursor: pointer;
  color: var(--text-secondary);
  background: var(--bg-surface);
  border-top: 1px solid var(--border);
}
.sync-strip:hover { background: var(--bg-hover); }
.sync-strip.syncing { color: var(--accent); }
.sync-strip.bad { color: var(--danger); }
.ctx-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
}
.ctx-menu {
  position: fixed;
  min-width: 150px;
  padding: 4px;
  background: var(--bg-surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.ctx-menu button {
  text-align: left;
  padding: 7px 10px;
  border-radius: 5px;
  font-size: var(--fs-12-5);
  color: var(--text-primary);
}
.ctx-menu button:hover { background: var(--bg-hover); }
.ctx-menu button.danger { color: var(--danger); }
</style>
