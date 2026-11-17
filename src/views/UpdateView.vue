<script lang="ts">
import { defineComponent } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useUpdateStore } from "../stores/update";
import UpdateModal from "../components/UpdateModal.vue";
export default defineComponent({
  components: { UpdateModal },
  data() { return { unlisten: null as UnlistenFn | null, error: "" }; },
  async mounted() {
    const refresh = async () => { try { await useUpdateStore().refresh(); } catch (e) { this.error = String(e); } };
    this.unlisten = await getCurrentWindow().onFocusChanged(({ payload }) => { if (payload) void refresh(); });
    await refresh();
  },
  beforeUnmount() { this.unlisten?.(); },
});
</script>
<template><div v-if="error" role="alert">{{ error }}</div><UpdateModal /></template>
