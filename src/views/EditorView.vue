<!--
  EditorView.vue — 编辑/新建窗口（独立）。
  含标题、文件夹下拉、标签多选、Markdown textarea、保存/取消/删除。
-->
<script lang="ts">
import { defineComponent } from "vue";
import { promptsGet, promptsCreate, promptsUpdate, promptsDelete } from "../api/prompts";
import { ensureBackendReady } from "../api";
import { foldersList } from "../api/folders";
import { tagsList, tagsCreate } from "../api/tags";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { Folder } from "../types/folder";
import type { Tag } from "../types/tag";
import MarkdownView from "../components/preview/MarkdownView.vue";
import RichEditor from "../components/RichEditor.vue";
import BaseToast from "../components/ui/BaseToast.vue";
import { Eye, EyeOff, X, Trash2, Check } from "lucide-vue-next";
import { useUIStore } from "../stores/ui";
import { confirmDanger } from "../utils/dialog";
import { useSettingsStore } from "../stores/settings";
import { applyPersistedTheme } from "../composables/useTheme";
import {
  listenAppEvent, EVT_THEME_CHANGED,
} from "../composables/useAppEvents";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { ThemeMode } from "../types/settings";
import { log } from "../utils/logger";

export default defineComponent({
  name: "EditorView",
  components: { MarkdownView, RichEditor, BaseToast, Eye, EyeOff, X, Trash2, Check },
  data() {
    return {
      isNew: true,
      promptId: null as number | null,
      title: "",
      content: "",
      richContent: null as Record<string, unknown> | null,
      wasRich: false,
      imageBusy: false,
      folderId: null as number | null,
      selectedTagIds: [] as number[],
      newTagInput: "",
      folders: [] as Folder[],
      tags: [] as Tag[],
      loaded: false,
      previewMode: false,
      dirty: false,
      unlisteners: [] as UnlistenFn[],
    };
  },
  computed: {
    canSave(): boolean { return this.title.trim().length > 0 && !this.imageBusy; },
    wordCount(): number { return this.content.length; },
    selectedTags(): Tag[] {
      return this.tags.filter((t) => this.selectedTagIds.includes(t.id));
    },
    unselectedTags(): Tag[] {
      return this.tags.filter((t) => !this.selectedTagIds.includes(t.id));
    },
  },
  async mounted() {
    log.info("EditorView mounted");
    await ensureBackendReady();
    const id = this.$route.params.id;
    if (id) {
      this.isNew = false;
      this.promptId = Number(id);
    }
    const settings = useSettingsStore();
    if (!settings.loaded) await settings.loadAll();
    applyPersistedTheme(settings.data.theme);
    await Promise.all([this.loadMeta(), this.loadPrompt()]);
    this.loaded = true;
    this.$nextTick(() => {
      const ref = this.isNew ? "titleInput" : "contentInput";
      (this.$refs[ref] as HTMLInputElement | HTMLTextAreaElement | undefined)?.focus();
    });
    document.addEventListener("keydown", this.onKey);
    this.unlisteners.push(
      await listenAppEvent<ThemeMode>(EVT_THEME_CHANGED, (m) => applyPersistedTheme(m)),
    );
  },
  beforeUnmount() {
    document.removeEventListener("keydown", this.onKey);
    for (const u of this.unlisteners) u();
  },
  watch: {
    title() { this.dirty = true; },
    content() { this.dirty = true; },
    richContent: { deep: true, handler() { this.dirty = true; } },
    folderId() { this.dirty = true; },
    selectedTagIds: { deep: true, handler() { this.dirty = true; } },
  },
  methods: {
    enableRich() {
      this.richContent = { type: "doc", content: this.content.split("\n").map((text) => ({ type: "paragraph", ...(text ? { content: [{ type: "text", text }] } : {}) })) };
    },
    async loadMeta() {
      [this.folders, this.tags] = await Promise.all([foldersList(), tagsList()]);
    },
    async loadPrompt() {
      if (!this.promptId) return;
      const p = await promptsGet(this.promptId);
      this.title = p.title;
      this.content = p.content;
      this.richContent = p.rich_content ?? null;
      this.wasRich = !!p.rich_content;
      this.folderId = p.folder_id;
      this.selectedTagIds = [...p.tag_ids];
      this.dirty = false;
    },
    addTag(id: number) {
      if (!this.selectedTagIds.includes(id)) this.selectedTagIds.push(id);
    },
    removeTag(id: number) {
      this.selectedTagIds = this.selectedTagIds.filter((x) => x !== id);
    },
    async createNewTag() {
      const name = this.newTagInput.trim().replace(/^#/, "");
      if (!name) return;
      const t = await tagsCreate(name);
      this.tags.push(t);
      this.addTag(t.id);
      this.newTagInput = "";
    },
    async save() {
      if (!this.canSave) {
        useUIStore().pushToast("标题不能为空", "warning");
        (this.$refs.titleInput as HTMLInputElement | undefined)?.focus();
        return;
      }
      const draft = {
        title: this.title.trim(),
        content: this.content,
        rich_content: this.richContent,
        folder_id: this.folderId,
        tag_ids: this.selectedTagIds,
      };
      try {
        if (this.isNew) {
          await promptsCreate(draft);
        } else if (this.promptId) {
          await promptsUpdate(this.promptId, draft);
        }
        this.dirty = false;
        useUIStore().pushToast("已保存", "success");
        getCurrentWebviewWindow().close();
      } catch (e) {
        useUIStore().pushToast(`保存失败: ${e}`, "danger");
      }
    },
    async cancel() {
      if (this.dirty && !(await confirmDanger("放弃未保存的修改？", "放弃修改"))) return;
      getCurrentWebviewWindow().close();
    },
    async remove() {
      if (!this.promptId) return;
      if (!(await confirmDanger(`确认删除「${this.title}」？此操作不可撤销。`, "删除提示词"))) return;
      try {
        await promptsDelete(this.promptId);
        getCurrentWebviewWindow().close();
      } catch (e) {
        useUIStore().pushToast(`删除失败: ${e}`, "danger");
      }
    },
    onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "s") {
        e.preventDefault();
        this.save();
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        this.cancel();
      }
    },
    onContentTab(e: KeyboardEvent) {
      // 在光标位置插入两个空格而非把焦点跳走 / 追加到末尾。
      e.preventDefault();
      const ta = e.target as HTMLTextAreaElement;
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      const insert = "  ";
      this.content =
        this.content.slice(0, start) + insert + this.content.slice(end);
      this.$nextTick(() => {
        ta.selectionStart = ta.selectionEnd = start + insert.length;
      });
    },
  },
});
</script>

<template>
  <div class="editor">
    <header class="head">
      <div class="hl">
        <span class="brand">{{ isNew ? "新建提示词" : "编辑提示词" }}</span>
        <span v-if="dirty" class="dirty">· 未保存</span>
      </div>
      <div class="hr">
        <button class="ghost" :disabled="imageBusy" @click="previewMode = !previewMode">
          <EyeOff v-if="previewMode" :size="13" />
          <Eye v-else :size="13" />
          {{ previewMode ? "返回编辑" : "预览" }}
        </button>
      </div>
    </header>

    <div v-if="loaded" class="body">
      <label class="field">
        <span class="label">标题</span>
        <input
          ref="titleInput"
          v-model="title"
          placeholder="给提示词起个名字"
          class="title-input"
        />
      </label>

      <div class="row">
        <label class="field">
          <span class="label">文件夹</span>
          <select v-model="folderId" class="select">
            <option :value="null">(未分类)</option>
            <option v-for="f in folders" :key="f.id" :value="f.id">{{ f.name }}</option>
          </select>
        </label>

        <label class="field">
          <span class="label">标签</span>
          <div class="tag-input">
            <span v-for="t in selectedTags" :key="t.id" class="chip">
              #{{ t.name }}
              <button @click="removeTag(t.id)"><X :size="10" /></button>
            </span>
            <select
              v-if="unselectedTags.length"
              class="add-tag"
              @change="(e) => { addTag(Number((e.target as HTMLSelectElement).value)); (e.target as HTMLSelectElement).value = ''; }"
            >
              <option value="">+ 选择</option>
              <option v-for="t in unselectedTags" :key="t.id" :value="t.id">
                {{ t.name }}
              </option>
            </select>
            <input
              v-model="newTagInput"
              placeholder="新建…"
              @keydown.enter.prevent="createNewTag"
              class="new-tag"
            />
          </div>
        </label>
      </div>

      <div class="field grow">
        <div class="content-label-row">
          <span class="label">{{ richContent ? "图文内容 · 仅本机" : "内容（Markdown）" }}</span>
          <button v-if="!richContent" class="ghost" @click="enableRich">切换图文</button>
          <span class="word">{{ wordCount }} 字<span v-if="!richContent"> · Tab 插入两空格</span></span>
        </div>
        <p v-if="richContent && !isNew && !wasRich" class="word">保存时另存图文副本，原文本保留。</p>
        <RichEditor v-if="richContent" :key="String(previewMode)" :readonly="previewMode" v-model="richContent" @text="content = $event" @busy="imageBusy = $event" />
        <MarkdownView v-else-if="previewMode" :source="content" />
        <textarea v-else
          ref="contentInput"
          v-model="content"
          @keydown.tab="onContentTab"
          class="content-input"
          spellcheck="false"
        />
      </div>
    </div>
    <div v-else class="loading">加载中…</div>

    <footer class="footer">
      <button v-if="!isNew" class="ghost danger" @click="remove">
        <Trash2 :size="13" /> 删除
      </button>
      <span class="spacer" />
      <button class="ghost" @click="cancel">取消</button>
      <button class="primary" :disabled="!canSave" @click="save">
        <Check :size="13" /> 保存 ⌘S
      </button>
    </footer>
    <BaseToast />
  </div>
</template>

<style scoped>
.editor {
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
  justify-content: space-between;
  border-bottom: 1px solid var(--border);
  background: var(--bg-titlebar);
  -webkit-app-region: drag;
}
.hl { display: flex; align-items: center; gap: 8px; }
.brand { font-weight: 600; font-size: var(--fs-13); }
.dirty { font-size: var(--fs-11); color: var(--warning); }
.hr { -webkit-app-region: no-drag; }
.body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 20px;
  overflow-y: auto;
}
.field { display: flex; flex-direction: column; gap: 6px; }
.field.grow { flex: 1; min-height: 0; }
.label {
  font-size: var(--fs-11);
  font-weight: 600;
  color: var(--text-secondary);
  letter-spacing: 0.4px;
}
.title-input {
  min-height: calc(40px * var(--font-scale));
  padding: 0 12px;
  border: 1.5px solid var(--accent);
  border-radius: 8px;
  background: var(--bg-input);
  font-size: var(--fs-14);
  font-weight: 500;
  color: var(--text-primary);
  box-shadow: var(--shadow-inner-input);
  outline: none;
}
.row { display: flex; flex-wrap: wrap; gap: 12px; }
.row .field { flex: 1 1 180px; min-width: 0; }
.select {
  min-height: calc(36px * var(--font-scale));
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: var(--fs-13);
}
.tag-input {
  min-height: 36px;
  padding: 4px 6px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-surface);
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
}
.chip {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  background: var(--accent-soft);
  color: var(--text-primary);
  padding: 3px 8px;
  border-radius: 4px;
  font-size: var(--fs-11);
}
.chip button { font-size: var(--fs-11); padding: 0 2px; color: var(--text-secondary); }
.add-tag, .new-tag {
  border: 0;
  background: transparent;
  font-size: var(--fs-11);
  color: var(--text-tertiary);
  outline: none;
}
.new-tag { width: 80px; }

.content-label-row {
  display: flex; align-items: center; justify-content: space-between;
}
.word { font-size: var(--fs-10); color: var(--text-tertiary); }
.content-input {
  flex: 1;
  min-height: 200px;
  padding: 14px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-input-disabled);
  font-family: var(--font-mono);
  font-size: var(--fs-13);
  line-height: 1.6;
  color: var(--text-primary);
  resize: none;
  outline: none;
}
.loading { padding: 40px; color: var(--text-tertiary); text-align: center; }
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
.spacer { flex: 1; }
.ghost {
  min-height: calc(32px * var(--font-scale)); padding: 0 14px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  font-size: var(--fs-12);
  color: var(--text-secondary);
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.ghost:hover { background: var(--bg-hover); }
.ghost.danger { border-color: transparent; color: var(--danger); }
.primary {
  min-height: calc(32px * var(--font-scale)); padding: 0 16px;
  border-radius: 6px;
  background: var(--accent);
  color: var(--accent-fg);
  font-size: var(--fs-12);
  font-weight: 600;
  box-shadow: var(--shadow-sm);
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.primary:disabled { opacity: 0.4; cursor: not-allowed; }
.primary:hover:not(:disabled) { opacity: 0.92; }
</style>
