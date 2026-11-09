<!--
  PreviewView.vue — 预览窗口（独立 WebviewWindow）。
  通过 IPC `prompts_get` 拉取并渲染 Markdown；底部"复制 / 注入"。
-->
<script lang="ts">
import { defineComponent } from "vue";
import { promptsGet } from "../api/prompts";
import { ensureBackendReady } from "../api";
import { foldersList } from "../api/folders";
import { tagsList } from "../api/tags";
import { promptsRecordUse } from "../api/prompts";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { Prompt } from "../types/prompt";
import type { Folder } from "../types/folder";
import type { Tag } from "../types/tag";
import { relativeTime } from "../utils/format";

import RichEditor from "../components/RichEditor.vue";
import { contentCopy } from "../api/media";
import MarkdownView from "../components/preview/MarkdownView.vue";
import BaseToast from "../components/ui/BaseToast.vue";
import {
  Eye, Folder as FolderIcon, Star, Pin, Copy, ArrowRight,
} from "lucide-vue-next";
import { useUIStore } from "../stores/ui";
import { useSettingsStore } from "../stores/settings";
import { applyPersistedTheme } from "../composables/useTheme";
import {
  listenAppEvent, EVT_THEME_CHANGED, EVT_PROMPTS_CHANGED,
} from "../composables/useAppEvents";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { ThemeMode } from "../types/settings";
import { log } from "../utils/logger";

export default defineComponent({
  name: "PreviewView",
  components: { RichEditor, MarkdownView, BaseToast, Eye, FolderIcon, Star, Pin, Copy, ArrowRight },
  data() {
    return {
      prompt: null as Prompt | null,
      folders: [] as Folder[],
      tags: [] as Tag[],
      error: "",
      unlisteners: [] as UnlistenFn[],
    };
  },
  computed: {
    folderName(): string {
      if (!this.prompt?.folder_id) return "(未分类)";
      return this.folders.find((f) => f.id === this.prompt!.folder_id)?.name ?? "—";
    },
    tagNames(): string[] {
      if (!this.prompt) return [];
      const map = new Map(this.tags.map((t) => [t.id, t.name]));
      return this.prompt.tag_ids.map((id) => map.get(id) ?? "?");
    },
    lastUsed(): string {
      return relativeTime(this.prompt?.last_used_at);
    },
    wordCount(): number {
      return this.prompt?.content.length ?? 0;
    },
  },
  async mounted() {
    log.info("PreviewView mounted");
    await ensureBackendReady();
    const settings = useSettingsStore();
    if (!settings.loaded) await settings.loadAll();
    applyPersistedTheme(settings.data.theme);

    const id = Number(this.$route.params.id);
    if (!id) {
      this.error = "缺少 prompt id";
      return;
    }
    await this.reload(id);
    document.addEventListener("keydown", this.onKey);
    this.unlisteners.push(
      await listenAppEvent<ThemeMode>(EVT_THEME_CHANGED, (m) => applyPersistedTheme(m)),
      await listenAppEvent(EVT_PROMPTS_CHANGED, () => this.reload(id)),
    );
  },
  beforeUnmount() {
    document.removeEventListener("keydown", this.onKey);
    for (const u of this.unlisteners) u();
  },
  methods: {
    async reload(id: number) {
      try {
        const [prompt, folders, tags] = await Promise.all([
          promptsGet(id), foldersList(), tagsList(),
        ]);
        this.prompt = prompt;
        this.folders = folders;
        this.tags = tags;
      } catch (e) {
        this.error = String(e);
      }
    },
    async copyOnly() {
      if (!this.prompt) return;
      await contentCopy(this.prompt.id, "prompt");
      await promptsRecordUse(this.prompt.id);
      useUIStore().pushToast("已复制到剪贴板", "success");
    },
    async inject() {
      if (!this.prompt) return;
      const r = await contentCopy(this.prompt.id, "prompt", true);
      if (r.ok) {
        await promptsRecordUse(this.prompt.id);
        getCurrentWebviewWindow().close();
      } else {
        useUIStore().pushToast(r.message ?? "注入失败 · 已复制到剪贴板", "warning");
      }
    },
    onKey(e: KeyboardEvent) {
      if (e.key === "Escape") getCurrentWebviewWindow().close();
      if (e.key === "Enter" && !((e.target as HTMLElement)?.tagName === "TEXTAREA")) {
        e.preventDefault();
        this.inject();
      }
    },
  },
});
</script>

<template>
  <div class="preview">
    <header class="head">
      <div class="hl">
        <Eye :size="14" class="hl-ico" />
        <span class="brand">预览</span>
        <span v-if="prompt" class="title-small">· {{ prompt.title }}</span>
      </div>
    </header>
    <div v-if="error" class="error">{{ error }}</div>
    <div v-else-if="prompt" class="meta">
      <div class="meta-line">
        <FolderIcon :size="11" />
        <span>{{ folderName }}</span>
        <span class="dot">·</span>
        <span>使用 {{ prompt.use_count }} 次</span>
        <span class="dot">·</span>
        <span>最近 {{ lastUsed }}</span>
        <span class="spacer" />
        <Star v-if="prompt.is_favorite" :size="13" :fill="'currentColor'" class="star" />
        <Pin v-if="prompt.is_pinned" :size="13" class="pin" />
      </div>
      <h1 class="title">{{ prompt.title }}</h1>
      <div class="tags">
        <span v-for="t in tagNames" :key="t" class="tag">#{{ t }}</span>
      </div>
    </div>
    <main v-if="prompt" class="body">
      <RichEditor v-if="prompt.rich_content" :model-value="prompt.rich_content" readonly />
      <MarkdownView v-else :source="prompt.content" />
    </main>
    <footer v-if="prompt" class="footer">
      <span class="stat">{{ wordCount }} 字</span>
      <span class="spacer" />
      <button class="btn" @click="copyOnly">
        <Copy :size="13" /> 复制
      </button>
      <button class="btn primary" @click="inject">
        <ArrowRight :size="13" /> 注入到当前窗口
      </button>
    </footer>
    <BaseToast />
  </div>
</template>

<style scoped>
.preview {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--bg-surface);
  color: var(--text-primary);
}
.head {
  min-height: calc(48px * var(--font-scale));
  padding: 0 16px;
  display: flex;
  align-items: center;
  border-bottom: 1px solid var(--border);
  background: var(--bg-titlebar);
  -webkit-app-region: drag;
}
.hl { display: flex; align-items: center; gap: 8px; }
.hl-ico { color: var(--accent); }
.brand { font-weight: 600; font-size: var(--fs-13); }
.title-small { font-size: var(--fs-11); color: var(--text-tertiary); }
.error { padding: 24px; color: var(--danger); }
.meta {
  padding: 16px 20px 14px;
  border-bottom: 1px solid var(--border);
}
.meta-line {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  font-size: var(--fs-11);
  color: var(--text-tertiary);
  margin-bottom: 8px;
}
.dot { color: var(--border-strong); }
.spacer { flex: 1; }
.star { color: var(--star); }
.pin { font-size: var(--fs-11); }
.title { font-size: var(--fs-18); font-weight: 700; }
.tags { display: flex; gap: 4px; margin-top: 8px; flex-wrap: wrap; }
.tag {
  font-size: var(--fs-10);
  background: var(--bg-hover);
  padding: 2px 6px;
  border-radius: 4px;
  color: var(--text-secondary);
}
.body { flex: 1; overflow-y: auto; padding: 16px 20px; }
.footer {
  display: flex;
  flex-wrap: wrap;
  flex-shrink: 0;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid var(--border);
  background: var(--bg-base);
}
.stat { font-size: var(--fs-11); color: var(--text-tertiary); }
.btn {
  min-height: calc(32px * var(--font-scale));
  padding: 0 12px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  font-size: var(--fs-12);
  font-weight: 500;
  color: var(--text-secondary);
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.btn:hover { background: var(--bg-hover); }
.btn.primary {
  background: var(--accent);
  color: var(--accent-fg);
  border: 0;
  font-weight: 600;
  padding: 0 14px;
  box-shadow: var(--shadow-sm);
}
.btn.primary:hover { opacity: 0.92; }
</style>
