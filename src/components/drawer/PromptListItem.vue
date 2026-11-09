<!--
  PromptListItem.vue — hover 显示「编辑 / 注入 / 收藏」。
  click 整行 = 选中；click 注入图标 = 直接注入。
-->
<script lang="ts">
import { defineComponent, type PropType } from "vue";
import { Star, ArrowRight, Pencil, Pin } from "lucide-vue-next";
import type { Prompt } from "../../types/prompt";
import { snippet } from "../../utils/format";
import { log } from "../../utils/logger";

export default defineComponent({
  name: "PromptListItem",
  components: { Star, ArrowRight, Pencil, Pin },
  props: {
    prompt: { type: Object as PropType<Prompt>, required: true },
    selected: { type: Boolean, default: false },
  },
  emits: {
    click: (_id: number) => true,
    "toggle-fav": (_id: number) => true,
    inject: (_id: number) => true,
    edit: (_id: number) => true,
    context: (_p: { id: number; x: number; y: number }) => true,
  },
  computed: {
    preview(): string { return snippet(this.prompt.content, 90); },
  },
  methods: {
    onRowClick() {
      log.info(`[ListItem] row click id=${this.prompt.id}`);
      this.$emit("click", this.prompt.id);
    },
    onContext(e: MouseEvent) {
      this.$emit("context", { id: this.prompt.id, x: e.clientX, y: e.clientY });
    },
    onStar(e: Event) {
      e.stopPropagation();
      log.info(`[ListItem] star click id=${this.prompt.id}`);
      this.$emit("toggle-fav", this.prompt.id);
    },
    onInject(e: Event) {
      e.stopPropagation();
      log.info(`[ListItem] inject click id=${this.prompt.id}`);
      this.$emit("inject", this.prompt.id);
    },
    onEdit(e: Event) {
      e.stopPropagation();
      log.info(`[ListItem] edit click id=${this.prompt.id}`);
      this.$emit("edit", this.prompt.id);
    },
  },
});
</script>

<template>
  <div class="item" :class="{ selected }" @click="onRowClick" @contextmenu.prevent="onContext">
    <div class="main">
      <span v-if="prompt.rich_content" class="meta">图文 · 仅本机</span>
      <div class="title-row">
        <Pin v-if="prompt.is_pinned" :size="11" class="pin" />
        <span class="title">{{ prompt.title }}</span>
      </div>
      <div class="content">{{ preview }}</div>
    </div>

    <div class="actions">
      <button
        type="button"
        class="ico-btn star-btn"
        :class="{ on: prompt.is_favorite }"
        @click="onStar"
        title="收藏"
      >
        <Star :size="14" :fill="prompt.is_favorite ? 'currentColor' : 'none'" />
      </button>
      <button
        type="button"
        class="ico-btn hover-only"
        @click="onEdit"
        title="编辑 (⌘E)"
      >
        <Pencil :size="14" />
      </button>
      <button
        type="button"
        class="ico-btn inject-btn hover-only"
        @click="onInject"
        title="注入到上一个输入框 (Enter)"
      >
        <ArrowRight :size="14" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.item {
  display: flex;
  gap: 8px;
  padding: 10px 12px;
  border-radius: 8px;
  cursor: pointer;
  position: relative;
  transition: background var(--dur-fast) var(--ease-out);
}
.item:hover { background: var(--bg-hover); }
/* 选中态：柔和整块高亮 + 细描边，区别于 hover；不再用生硬的彩色左边条。 */
.item.selected {
  background: var(--bg-selected);
  box-shadow: inset 0 0 0 1px var(--border-strong);
}
.item.selected .title { color: var(--text-primary); }
.main { flex: 1; min-width: 0; }
.title-row {
  display: flex; gap: 4px; align-items: center;
  margin-bottom: 2px;
}
.pin { color: var(--text-tertiary); }
.title {
  font-size: var(--fs-13);
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.content {
  font-size: var(--fs-11);
  color: var(--text-secondary);
  line-height: 1.4;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.actions {
  display: flex;
  gap: 2px;
  align-items: flex-start;
  flex-shrink: 0;
}
.ico-btn {
  width: 26px; min-height: calc(26px * var(--font-scale));
  display: flex; align-items: center; justify-content: center;
  border-radius: 5px;
  color: var(--text-tertiary);
  transition: opacity var(--dur-fast) var(--ease-out),
              background var(--dur-fast) var(--ease-out),
              color var(--dur-fast) var(--ease-out);
}
/* 关键：让 svg 不接收点击，所有点击都直接落在 button 上。
   否则 webview 在某些场景里把 e.target 锁定到 svg/path，
   button 的 @click bubble 链路被打断。 */
.ico-btn svg, .ico-btn svg * { pointer-events: none; }

.ico-btn:hover { background: var(--bg-surface); color: var(--text-primary); }
.star-btn.on { color: var(--star); }
.inject-btn:hover {
  background: var(--accent);
  color: var(--accent-fg);
}
.hover-only { opacity: 0; }
.item:hover .hover-only,
.item.selected .hover-only { opacity: 1; }
</style>
