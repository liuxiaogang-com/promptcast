<!-- SearchBar.vue — 顶栏。整体可拖拽（除按钮 / 搜索框）。 -->
<script lang="ts">
import { defineComponent } from "vue";
import {
  Sparkle, Pin, ArrowUpDown, Settings, Plus, Search,
} from "lucide-vue-next";
import { useUIStore } from "../../stores/ui";
import { usePromptsStore } from "../../stores/prompts";
import {
  windowOpenEditor,
  windowOpenSettings,
  windowSetPin,
} from "../../api/window";
import type { SortMode } from "../../types/prompt";
import { modKey } from "../../utils/format";

export default defineComponent({
  name: "SearchBar",
  components: { Sparkle, Pin, ArrowUpDown, Settings, Plus, Search },
  props: {
    modelValue: { type: String, required: true },
  },
  emits: {
    "update:modelValue": (_v: string) => true,
  },
  data() {
    return {
      sortMenuOpen: false,
      modSymbol: modKey(),
    };
  },
  computed: {
    ui() { return useUIStore(); },
    prompts() { return usePromptsStore(); },
  },
  mounted() {
    document.addEventListener("click", this.onDocClick);
  },
  beforeUnmount() {
    document.removeEventListener("click", this.onDocClick);
  },
  methods: {
    onInput(e: Event) {
      this.$emit("update:modelValue", (e.target as HTMLInputElement).value);
    },
    async togglePin() {
      this.ui.drawerPinned = !this.ui.drawerPinned;
      await windowSetPin(this.ui.drawerPinned);
    },
    async openSettings() { await windowOpenSettings(); },
    async openNew() { await windowOpenEditor(null); },
    async setSort(mode: SortMode) {
      this.sortMenuOpen = false;
      await this.prompts.setSort(mode);
    },
    onDocClick(e: MouseEvent) {
      if (!this.sortMenuOpen) return;
      const wrap = (this.$el as HTMLElement)?.querySelector(".sort-wrap");
      if (wrap && !wrap.contains(e.target as Node)) this.sortMenuOpen = false;
    },
  },
});
</script>

<template>
  <header class="search-bar">
    <div class="title-row">
      <div class="brand">
        <Sparkle :size="14" class="brand-ico" />
        <span class="name">PromptCast</span>
      </div>
      <div class="actions">
        <button
          class="icon-btn"
          :class="{ active: ui.drawerPinned }"
          @click="togglePin"
          title="钉住（不自动隐藏）"
        >
          <Pin :size="14" />
        </button>
        <div class="sort-wrap">
          <button class="icon-btn" @click="sortMenuOpen = !sortMenuOpen" title="排序">
            <ArrowUpDown :size="14" />
          </button>
          <div v-if="sortMenuOpen" class="sort-menu">
            <button @click="setSort('recent_used')">最近使用</button>
            <button @click="setSort('created')">创建时间</button>
            <button @click="setSort('updated')">更新时间</button>
            <button @click="setSort('title')">标题 A-Z</button>
          </div>
        </div>
        <button class="icon-btn" @click="openSettings" title="设置">
          <Settings :size="14" />
        </button>
        <span class="divider" />
        <button class="primary-btn" @click="openNew" title="新建">
          <Plus :size="13" />
          <span>新建</span>
        </button>
      </div>
    </div>
    <div class="search-row">
      <label class="search-field">
        <Search :size="14" class="search-icon" aria-hidden="true" />
        <input
          :value="modelValue"
          @input="onInput"
          aria-label="搜索提示词"
          placeholder="搜索提示词…"
        />
        <span class="hint" aria-hidden="true">{{ modSymbol }}F</span>
      </label>
    </div>
  </header>
</template>

<style scoped>
.search-bar {
  display: flex;
  flex-direction: column;
  background: var(--bg-titlebar);
}
.title-row {
  min-height: calc(44px * var(--font-scale));
  padding: 0 8px 0 12px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  -webkit-app-region: drag;
}
.brand { display: flex; align-items: center; gap: 8px; }
.brand-ico { color: var(--accent); }
.brand .name { font-weight: 700; font-size: var(--fs-13); letter-spacing: 0.2px; }

.actions {
  display: flex; align-items: center; gap: 2px;
  -webkit-app-region: no-drag;
}
.icon-btn {
  width: 28px; min-height: calc(28px * var(--font-scale));
  display: flex; align-items: center; justify-content: center;
  border-radius: 6px;
  color: var(--text-secondary);
  transition: background var(--dur-fast) var(--ease-out);
}
.icon-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
.icon-btn.active { background: var(--accent-soft); color: var(--accent); }
.divider { width: 1px; height: 16px; background: var(--border); margin: 0 4px; }

.primary-btn {
  min-height: calc(28px * var(--font-scale));
  padding: 0 12px;
  display: flex; align-items: center; gap: 4px;
  background: var(--accent);
  color: var(--accent-fg);
  border-radius: 6px;
  font-size: var(--fs-12);
  font-weight: 600;
  box-shadow: var(--shadow-sm);
}
.primary-btn:hover { opacity: 0.92; }

.search-row {
  padding: 4px 12px 10px;
  -webkit-app-region: no-drag;
}
.search-field {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: calc(36px * var(--font-scale));
  padding: 0 10px;
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: 8px;
  box-shadow: var(--shadow-inner-input);
  cursor: text;
}
.search-field:focus-within { border-color: var(--text-secondary); }
.search-field input {
  flex: 1;
  min-width: 0;
  width: 100%;
  min-height: calc(34px * var(--font-scale));
  padding: 0;
  background: transparent;
  border: 0;
  outline: none;
  color: var(--text-primary);
  font-size: var(--fs-13);
}
.search-icon { color: var(--text-tertiary); flex-shrink: 0; }
.hint {
  font-family: var(--font-mono);
  font-size: var(--fs-10);
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.sort-wrap { position: relative; }
.sort-menu {
  position: absolute;
  top: 100%; right: 0;
  background: var(--bg-surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  box-shadow: var(--shadow-md);
  padding: 4px;
  display: flex; flex-direction: column;
  min-width: 120px;
  z-index: 10;
}
.sort-menu button {
  text-align: left;
  padding: 6px 8px;
  border-radius: 4px;
  font-size: var(--fs-12);
  color: var(--text-primary);
}
.sort-menu button:hover { background: var(--bg-hover); }
</style>
