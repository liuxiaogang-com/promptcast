<script lang="ts">
import { defineComponent } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import ReleaseNotes from "../components/ReleaseNotes.vue";
import { ensureBackendReady } from "../api";
import { useSettingsStore } from "../stores/settings";
import { applyPersistedTheme } from "../composables/useTheme";
import { listenAppEvent, EVT_THEME_CHANGED } from "../composables/useAppEvents";
import type { ThemeMode } from "../types/settings";

export default defineComponent({
  name: "ChangelogView",
  components: { ReleaseNotes },
  data() {
    return { opened: false, unlisteners: [] as UnlistenFn[] };
  },
  async mounted() {
    await ensureBackendReady();
    const settings = useSettingsStore();
    if (!settings.loaded) await settings.loadAll();
    applyPersistedTheme(settings.data.theme);
    const win = getCurrentWebviewWindow();
    this.unlisteners.push(
      await listenAppEvent<ThemeMode>(EVT_THEME_CHANGED, (m) => applyPersistedTheme(m)),
      await win.onFocusChanged(({ payload }) => {
        if (payload) this.opened = true;
      }),
    );
    // 预声明窗口启动时隐藏，首次打开后才加载日志。
    if (await win.isVisible()) this.opened = true;
  },
  beforeUnmount() {
    for (const u of this.unlisteners) u();
  },
});
</script>

<template>
  <main class="changelog">
    <ReleaseNotes v-if="opened" />
  </main>
</template>

<style scoped>
.changelog {
  min-height: 100vh;
  padding: 20px 24px;
  background: var(--bg-surface);
  color: var(--text-primary);
  overflow-wrap: anywhere;
}
.changelog :deep(.release-notes) { margin-top: 0; }
</style>
