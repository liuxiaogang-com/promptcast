<!--
  ClipboardList.vue — 剪贴板历史列表（「剪贴板」分类 chip 选中时显示）。
  交互：每项前常驻多选框；鼠标点击行/勾选框 = 多选；选中≥1 时顶部出现工具条
  （合并粘贴 / 批量复制 / 批量删除）。键盘上下移动高亮、回车注入由父组件 DrawerView 处理。
  单条操作：悬停出现注入按钮、垃圾桶删除按钮。
-->
<script lang="ts">
import { defineComponent, markRaw, type PropType } from "vue";
import LocalImage from "../LocalImage.vue";
import { Trash2, ArrowRight, Star, Save } from "lucide-vue-next";
import type { ClipEntry } from "../../types/clipboard";
import { snippet, relativeTime } from "../../utils/format";

export default defineComponent({
  name: "ClipboardList",
  components: { LocalImage, Trash2, ArrowRight, Star, Save },
  props: {
    entries: { type: Array as PropType<ClipEntry[]>, required: true },
    resetToken: { type: Number, default: 0 },
    selectedId: { type: Number as PropType<number | null>, default: null },
  },
  emits: {
    save: (_id: number) => true,
    favorite: (_id: number) => true,
    inject: (_id: number) => true,
    delete: (_id: number) => true,
    bulkDelete: (_ids: number[]) => true,
    bulkPaste: (_ids: number[]) => true,
    bulkCopy: (_ids: number[]) => true,
  },
  data() {
    return { checked: [] as number[], scrollTop: 0, viewportHeight: 600, scale: 1, observer: null as ResizeObserver | null };
  },
  computed: {
    textOnly(): boolean { return !this.entries.some((c) => this.checked.includes(c.id) && c.image_id); },
    rows(): { entry: ClipEntry; top: number; height: number }[] {
      let top = 0;
      return this.entries.map((entry) => {
        const height = (entry.image_id ? 184 : 88) * this.scale;
        const row = { entry, top, height };
        top += height;
        return row;
      });
    },
    totalHeight(): number { const last = this.rows[this.rows.length - 1]; return last ? last.top + last.height : 0; },
    visibleRows(): { entry: ClipEntry; top: number; height: number }[] {
      const margin = 3 * 184 * this.scale;
      return this.rows.filter((r) => r.top + r.height > this.scrollTop - margin && r.top < this.scrollTop + this.viewportHeight + margin);
    },
  },
  mounted() {
    this.observer = markRaw(new ResizeObserver(() => this.measure()));
    this.observer.observe(this.$refs.viewport as HTMLElement);
    this.observer.observe(this.$refs.measure as HTMLElement);
    this.measure();
  },
  beforeUnmount() { this.observer?.disconnect(); },
  watch: {
    resetToken() { this.resetScroll(); },
    selectedId() { this.$nextTick(() => this.revealSelected()); },
    // 列表变化（如删除后重载）时，剔除已不存在的勾选项，避免“幽灵选中”。
    entries(list: ClipEntry[], previous: ClipEntry[]) {
      if (list[0]?.id !== previous[0]?.id) this.resetScroll();
      const ids = new Set(list.map((c) => c.id));
      this.checked = this.checked.filter((id) => ids.has(id));
    },
  },
  methods: {
    measure() {
      const viewport = this.$refs.viewport as HTMLElement;
      const height = (this.$refs.measure as HTMLElement).getBoundingClientRect().height;
      if (height > 0) this.scale = height / 100;
      if (viewport.clientHeight > 0) this.viewportHeight = viewport.clientHeight;
      this.setScroll(this.scrollTop);
    },
    setScroll(top: number) {
      const viewport = this.$refs.viewport as HTMLElement | undefined;
      this.scrollTop = Math.max(0, Math.min(top, this.totalHeight - this.viewportHeight));
      if (viewport) viewport.scrollTop = this.scrollTop;
    },
    resetScroll() { this.setScroll(0); },
    onScroll(event: Event) { this.scrollTop = (event.target as HTMLElement).scrollTop; },
    revealSelected() {
      const row = this.rows.find((r) => r.entry.id === this.selectedId);
      if (!row) return;
      if (row.top < this.scrollTop) this.setScroll(row.top);
      else if (row.top + row.height > this.scrollTop + this.viewportHeight) this.setScroll(row.top + row.height - this.viewportHeight);
    },
    preview(c: ClipEntry): string { return snippet(c.content, 100); },
    time(c: ClipEntry): string { return relativeTime(c.created_at); },
    isChecked(id: number): boolean { return this.checked.includes(id); },
    toggle(id: number): void {
      const i = this.checked.indexOf(id);
      if (i >= 0) this.checked.splice(i, 1);
      else this.checked.push(id);
    },
    clearChecked(): void { this.checked = []; },
    // 按列表显示顺序回传选中的 id，便于父组件按序拼接内容。
    orderedChecked(): number[] {
      return this.entries.filter((c) => this.checked.includes(c.id)).map((c) => c.id);
    },
    bulkDelete(): void { this.$emit("bulkDelete", this.orderedChecked()); },
    bulkPaste(): void { this.$emit("bulkPaste", this.orderedChecked()); this.clearChecked(); },
    bulkCopy(): void { this.$emit("bulkCopy", this.orderedChecked()); this.clearChecked(); },
  },
});
</script>

<template>
  <div class="list">
    <div v-if="checked.length" class="bulkbar">
      <span class="cnt">已选 {{ checked.length }}</span>
      <button type="button" class="tbtn" @click="clearChecked">取消</button>
      <span class="spacer" />
      <button v-if="textOnly" type="button" class="tbtn" @click="bulkPaste">合并粘贴</button>
      <button v-if="textOnly" type="button" class="tbtn" @click="bulkCopy">批量复制</button>
      <button type="button" class="tbtn danger" @click="bulkDelete">删除</button>
    </div>

    <div ref="viewport" class="viewport" @scroll.passive="onScroll">
    <div ref="measure" class="measure" aria-hidden="true" />
    <div class="virtual-space" :style="{ height: `${totalHeight}px` }">
    <div v-for="{ entry: c, top, height } in visibleRows" :key="c.id" :data-id="c.id" class="virtual-row" :style="{ top: `${top}px`, height: `${height}px` }">
      <div
        class="item"
        :class="{ selected: c.id === selectedId, checked: isChecked(c.id) }"
        @click="toggle(c.id)"
      >
        <input
          type="checkbox"
          class="cbx"
          :checked="isChecked(c.id)"
          @click.stop="toggle(c.id)"
          title="多选"
        />
        <div class="main">
          <LocalImage v-if="c.image_id" :id="c.image_id" />
          <div v-else class="text">{{ preview(c) }}</div>
          <div class="meta">{{ time(c) }} · {{ c.image_id ? "图片 · 仅本机" : `${c.char_count} 字` }}{{ c.saved_prompt_id ? " · 已保存" : "" }}</div>
        </div>
        <div class="actions">
          <button class="ico-btn" @click.stop="$emit('save', c.id)" :title="c.saved_prompt_id ? '打开已保存内容' : '长期保存'"><Save :size="14" /></button>
          <button class="ico-btn" @click.stop="$emit('favorite', c.id)" title="保存并收藏"><Star :size="14" /></button>
          <button
            type="button"
            class="ico-btn hover-only"
            @click.stop="$emit('inject', c.id)"
            title="注入到上一个输入框 (Enter)"
          >
            <ArrowRight :size="14" />
          </button>
          <button
            type="button"
            class="ico-btn hover-only del"
            @click.stop="$emit('delete', c.id)"
            title="删除"
          >
            <Trash2 :size="14" />
          </button>
        </div>
      </div>
    </div>
    </div>
    <div v-if="!entries.length" class="empty">
      <p>暂无剪贴板记录</p>
      <p class="hint">复制文本或图片后会自动出现在这里</p>
    </div>
    </div>
  </div>
</template>

<style scoped>
.list { flex: 1; min-height: 0; display: flex; flex-direction: column; padding: 4px 8px 8px; }
.viewport { flex: 1; min-height: 0; overflow-y: auto; overflow-anchor: none; position: relative; }
.measure { position: absolute; width: 0; height: calc(100px * var(--font-scale)); visibility: hidden; pointer-events: none; }
.virtual-space { position: relative; }
.virtual-row { position: absolute; left: 0; right: 0; }
.virtual-row .item { height: 100%; box-sizing: border-box; overflow: hidden; }
.main :deep(.local-image) { max-height: calc(130px * var(--font-scale)); }
.meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.bulkbar {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 6px;
  margin-bottom: 4px;
  background: var(--bg-titlebar);
  border: 1px solid var(--border);
  border-radius: 8px;
}
.bulkbar .cnt { font-size: var(--fs-11); color: var(--text-secondary); }
.bulkbar .spacer { flex: 1; }
.tbtn {
  min-height: calc(24px * var(--font-scale));
  padding: 0 10px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  color: var(--text-secondary);
  font-size: var(--fs-11);
}
.tbtn:hover { background: var(--bg-hover); color: var(--text-primary); }
.tbtn.danger { color: var(--danger); border-color: transparent; }
.item {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  padding: 10px 12px;
  border-radius: 8px;
  cursor: pointer;
  position: relative;
  transition: background var(--dur-fast) var(--ease-out);
}
.item:hover { background: var(--bg-hover); }
.item.selected {
  background: var(--bg-selected);
  box-shadow: inset 0 0 0 1px var(--border-strong);
}
.item.checked { background: var(--accent-soft); }
.cbx {
  margin-top: 2px;
  width: 14px;
  height: 14px;
  flex-shrink: 0;
  cursor: pointer;
  accent-color: var(--accent);
}
.main { flex: 1; min-width: 0; }
.text {
  font-size: var(--fs-12-5);
  color: var(--text-primary);
  line-height: 1.45;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  word-break: break-word;
}
.meta { font-size: var(--fs-10); color: var(--text-tertiary); margin-top: 3px; }
.actions { display: flex; gap: 2px; align-items: flex-start; flex-shrink: 0; }
.ico-btn {
  width: 26px; min-height: calc(26px * var(--font-scale));
  display: flex; align-items: center; justify-content: center;
  border-radius: 5px;
  color: var(--text-tertiary);
  transition: opacity var(--dur-fast) var(--ease-out),
              background var(--dur-fast) var(--ease-out),
              color var(--dur-fast) var(--ease-out);
}
.ico-btn svg, .ico-btn svg * { pointer-events: none; }
.ico-btn:hover { background: var(--bg-surface); color: var(--text-primary); }
.ico-btn.del:hover { color: var(--danger); }
.hover-only { opacity: 0; }
.item:hover .hover-only,
.item.selected .hover-only { opacity: 1; }
.empty {
  padding: 50px 16px;
  text-align: center;
  color: var(--text-tertiary);
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.empty .hint { font-size: var(--fs-11); }
</style>
