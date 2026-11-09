<script lang="ts">
import { defineComponent } from "vue";
import { updateChangelog } from "../api/update";
import type { ReleaseNotes } from "../types/update";
import MarkdownView from "./preview/MarkdownView.vue";

export default defineComponent({
  name: "ReleaseNotes",
  components: { MarkdownView },
  props: {
    version: { type: String, default: "" },
  },
  data() {
    return { releases: [] as ReleaseNotes[], loading: false, error: "" };
  },
  computed: {
    entries(): ReleaseNotes[] {
      return this.version
        ? this.releases.filter((r) => r.version.replace(/^v/, "") === this.version.replace(/^v/, ""))
        : this.releases;
    },
  },
  mounted() { void this.load(); },
  methods: {
    async load(): Promise<void> {
      if (this.loading) return;
      this.loading = true;
      this.error = "";
      try {
        this.releases = await updateChangelog();
      } catch {
        this.error = "版本日志暂不可用，不影响检查和安装更新。";
      } finally {
        this.loading = false;
      }
    },
  },
});
</script>

<template>
  <div class="release-notes" aria-live="polite">
    <p v-if="loading" class="hint">正在加载版本日志…</p>
    <p v-else-if="error" class="hint">
      {{ error }} <button @click="load">重试</button>
    </p>
    <p v-else-if="!entries.length" class="hint">暂无该版本的日志。</p>
    <section v-for="entry in entries" v-else :key="entry.version" class="entry">
      <h4>v{{ entry.version }} · {{ entry.title }}</h4>
      <div v-if="entry.pub_date" class="hint">{{ entry.pub_date }}</div>
      <MarkdownView :source="entry.notes" />
    </section>
  </div>
</template>

<style scoped>
.release-notes { margin-top: 12px; }
.entry + .entry { margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--border); }
h4 { margin: 0 0 4px; font-size: 13px; color: var(--text-primary); }
.hint { font-size: 12px; color: var(--text-tertiary); }
button { color: var(--accent); background: transparent; border: 0; cursor: pointer; }
</style>
