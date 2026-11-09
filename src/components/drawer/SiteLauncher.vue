<!-- SiteLauncher.vue — 底部网址栏。 -->
<script lang="ts">
import { defineComponent } from "vue";
import { Plus } from "lucide-vue-next";
import { useSitesStore } from "../../stores/sites";
import { windowOpenSettings } from "../../api/window";

export default defineComponent({
  name: "SiteLauncher",
  components: { Plus },
  computed: {
    sites() { return useSitesStore().list; },
  },
  methods: {
    async open(id: number) {
      await useSitesStore().open(id);
    },
    async addSite() {
      await windowOpenSettings();
    },
    initial(name: string): string {
      return name.trim().charAt(0).toUpperCase() || "?";
    },
  },
});
</script>

<template>
  <div class="site-row">
    <button
      v-for="s in sites"
      :key="s.id"
      class="site-btn"
      :title="`${s.name} · ${s.url}`"
      @click="open(s.id)"
    >
      <img v-if="s.favicon_data_uri" :src="s.favicon_data_uri" alt="" />
      <span v-else class="placeholder">{{ initial(s.name) }}</span>
    </button>
    <span class="spacer" />
    <button class="add-btn" @click="addSite" title="添加网址">
      <Plus :size="14" />
    </button>
  </div>
</template>

<style scoped>
.site-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  background: var(--bg-titlebar);
  border-top: 1px solid var(--border);
}
.site-btn {
  width: 30px; min-height: calc(30px * var(--font-scale));
  border-radius: 7px;
  display: flex; align-items: center; justify-content: center;
  background: var(--bg-surface);
  box-shadow: var(--shadow-sm);
  overflow: hidden;
  transition: transform var(--dur-fast) var(--ease-out);
}
.site-btn:hover { transform: translateY(-1px); }
.site-btn img { width: 18px; height: 18px; object-fit: contain; }
.site-btn .placeholder {
  font-size: var(--fs-12);
  font-weight: 700;
  color: var(--text-secondary);
}
.spacer { flex: 1; }
.add-btn {
  width: 30px; min-height: calc(30px * var(--font-scale));
  border-radius: 7px;
  border: 1px dashed var(--border-strong);
  color: var(--text-tertiary);
  font-size: var(--fs-16);
  display: flex; align-items: center; justify-content: center;
}
.add-btn:hover { color: var(--text-primary); }
</style>
