<!-- MarkdownView.vue — 安全的 Markdown 渲染。 -->
<script lang="ts">
import { defineComponent } from "vue";
import { renderMarkdown } from "../../utils/markdown";

export default defineComponent({
  name: "MarkdownView",
  props: {
    source: { type: String, required: true },
  },
  computed: {
    html(): string {
      return renderMarkdown(this.source);
    },
  },
});
</script>

<template>
  <article class="md" v-html="html" />
</template>

<style scoped>
.md {
  font-size: var(--fs-13);
  line-height: 1.7;
  color: var(--text-primary);
  word-break: break-word;
  user-select: text;
  cursor: text;
}
.md :deep(h1), .md :deep(h2), .md :deep(h3) {
  margin: 16px 0 6px;
  font-weight: 700;
}
.md :deep(h1) { font-size: var(--fs-18); }
.md :deep(h2) { font-size: var(--fs-15); }
.md :deep(h3) { font-size: var(--fs-13); }
.md :deep(p) { margin: 8px 0; }
.md :deep(ul), .md :deep(ol) { padding-left: 20px; margin: 8px 0; }
.md :deep(li) { margin: 2px 0; }
.md :deep(a) { color: var(--accent); text-decoration: underline; }
.md :deep(code) {
  font-family: var(--font-mono);
  background: var(--bg-hover);
  padding: 2px 5px;
  border-radius: 4px;
  font-size: var(--fs-11-5);
}
.md :deep(pre) {
  background: var(--bg-hover);
  padding: 12px;
  border-radius: 6px;
  overflow-x: auto;
  margin: 10px 0;
}
.md :deep(pre code) {
  background: transparent;
  padding: 0;
  font-size: var(--fs-12);
}
.md :deep(blockquote) {
  border-left: 3px solid var(--border-strong);
  padding-left: 12px;
  margin: 8px 0;
  color: var(--text-secondary);
}
.md :deep(hr) {
  border: 0;
  border-top: 1px solid var(--border);
  margin: 16px 0;
}
</style>
