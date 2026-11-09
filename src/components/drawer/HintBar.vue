<!-- HintBar.vue — 底部快捷键提示。 -->
<script lang="ts">
import { defineComponent } from "vue";
import { CornerDownLeft } from "lucide-vue-next";
import { modKey } from "../../utils/format";

export default defineComponent({
  name: "HintBar",
  components: { CornerDownLeft },
  props: {
    count: { type: Number, default: 0 },
  },
  data() {
    return { mod: modKey() };
  },
});
</script>

<template>
  <div class="hint-bar">
    <span class="kbd">
      <CornerDownLeft :size="11" />
      注入
    </span>
    <span class="dot">·</span>
    <span class="kbd">{{ mod }}C 复制</span>
    <span class="dot">·</span>
    <span class="kbd">{{ mod }}E 编辑</span>
    <span class="dot">·</span>
    <span class="kbd">Space 预览</span>
    <span class="spacer" />
    <span class="count">{{ count }} 条</span>
  </div>
</template>

<style scoped>
.hint-bar {
  display: flex;
  flex-wrap: wrap;
  flex-shrink: 0;
  align-items: center;
  gap: 6px;
  min-height: calc(28px * var(--font-scale));
  padding: 0 12px;
  background: var(--bg-surface);
  border-top: 1px solid var(--border);
  font-size: var(--fs-10);
  color: var(--text-secondary);
}
.kbd { white-space: nowrap; display: inline-flex; align-items: center; gap: 3px; }
.dot { color: var(--border-strong); }
.spacer { flex: 1; }
.count { color: var(--text-tertiary); }
</style>
