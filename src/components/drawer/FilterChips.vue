<!-- FilterChips.vue — 横向 chip 筛选；Tab/Shift+Tab 循环。 -->
<script lang="ts">
import { defineComponent, markRaw, type Component, type PropType } from "vue";
import { Star, Clipboard } from "lucide-vue-next";
import { useUIStore } from "../../stores/ui";
import { useFoldersStore } from "../../stores/folders";
import type { Folder } from "../../types/folder";

interface Chip {
  key: string;
  label: string;
  icon?: Component; // lucide 图标组件（不用 emoji）
}

export default defineComponent({
  name: "FilterChips",
  props: {
    counts: {
      type: Object as PropType<Record<string, number>>,
      default: () => ({}),
    },
  },
  computed: {
    ui() { return useUIStore(); },
    chips(): Chip[] {
      // 只放固定项 + 文件夹分类；标签不再进分类条（标签多了会把这一行撑得很长）。
      const folders = useFoldersStore().list as Folder[];
      return [
        { key: "all", label: "全部" },
        { key: "favorites", label: "收藏", icon: markRaw(Star) },
        { key: "clipboard", label: "剪贴板", icon: markRaw(Clipboard) },
        ...folders.map((f) => ({ key: `folder:${f.id}`, label: f.name })),
      ];
    },
    activeIdx(): number {
      return Math.max(0, this.chips.findIndex((c) => c.key === this.ui.activeChipKey));
    },
  },
  watch: {
    // 删除当前筛选的文件夹/标签后，对应 chip 消失：把选中回退到「全部」，
    // 否则列表会基于已删 id 过滤成空、高亮也对不上。
    chips(list: Chip[]) {
      if (!list.find((c) => c.key === this.ui.activeChipKey)) {
        this.ui.activeChipKey = "all";
      }
    },
    // 选中项变化（Tab 循环 / 点击）时，把它滚进可视区，使超出窗口的分类也能看到。
    activeIdx() {
      this.$nextTick(() => this.scrollActiveIntoView());
    },
  },
  mounted() {
    document.addEventListener("keydown", this.onKey, true);
  },
  beforeUnmount() {
    document.removeEventListener("keydown", this.onKey, true);
  },
  methods: {
    onKey(e: KeyboardEvent) {
      if (e.key !== "Tab") return;
      // Tab 仅在 chip 间循环；如果焦点在输入框 / textarea 内仍允许
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === "TEXTAREA" || tag === "INPUT") return; // 输入框里 Tab 不拦截
      e.preventDefault();
      const len = this.chips.length;
      if (len === 0) return;
      const dir = e.shiftKey ? -1 : 1;
      const next = (this.activeIdx + dir + len) % len;
      this.ui.activeChipKey = this.chips[next].key;
    },
    select(key: string) {
      this.ui.activeChipKey = key;
    },
    scrollActiveIntoView() {
      const el = (this.$el as HTMLElement)?.querySelector?.(".chip.active") as HTMLElement | null;
      el?.scrollIntoView({ inline: "nearest", block: "nearest" });
    },
    countOf(key: string): number | null {
      const v = this.counts[key];
      return Number.isFinite(v) ? v : null;
    },
  },
});
</script>

<template>
  <div class="chips">
    <button
      v-for="c in chips"
      :key="c.key"
      class="chip"
      :class="{ active: c.key === ui.activeChipKey }"
      @click="select(c.key)"
    >
      <component :is="c.icon" v-if="c.icon" :size="12" class="ico" />
      <span>{{ c.label }}</span>
      <span v-if="countOf(c.key) !== null" class="count">{{ countOf(c.key) }}</span>
    </button>
  </div>
</template>

<style scoped>
.chips {
  display: flex;
  gap: 6px;
  padding: 0 12px 8px;
  flex-shrink: 0;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: thin; /* Firefox：细滚动条 */
  background: var(--bg-titlebar);
}
/* 分类多到超出窗口时显示一条细横向滚动条，可左右滑动看到后面的分类。 */
.chips::-webkit-scrollbar { height: 6px; }
.chips::-webkit-scrollbar-thumb {
  background: var(--border-strong);
  border-radius: 3px;
}
.chips::-webkit-scrollbar-track { background: transparent; }

.chip {
  flex-shrink: 0;
  min-height: calc(24px * var(--font-scale));
  padding: 0 10px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  border-radius: 999px;
  background: var(--bg-surface);
  border: 1px solid var(--border);
  color: var(--text-secondary);
  font-size: var(--fs-11);
  font-weight: 500;
  transition: all var(--dur-fast) var(--ease-out);
}
.chip:hover { background: var(--bg-hover); }
.chip.active {
  background: var(--accent);
  color: var(--accent-fg);
  border-color: var(--accent);
  font-weight: 600;
  box-shadow: var(--shadow-sm);
}
.chip .ico { font-size: var(--fs-10); }
.chip .count { color: var(--text-tertiary); font-size: var(--fs-10); }
.chip.active .count { color: var(--accent-fg); opacity: 0.7; }
</style>
