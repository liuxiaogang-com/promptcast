<script lang="ts">
import { defineComponent } from "vue";
import { imageGet } from "../api/media";
export default defineComponent({
  props: { id: { type: String, required: true } },
  data() { return { src: "", error: "" }; },
  watch: { id: { immediate: true, async handler(id: string) {
    this.src = ""; this.error = "";
    try { const asset = await imageGet(id, true); if (this.id === id) this.src = asset.data_url; }
    catch { if (this.id === id) this.error = "图片暂不可用"; }
  } } },
});
</script>
<template>
  <img v-if="src" :src="src" alt="图片" class="local-image" />
  <span v-else>{{ error || "加载图片…" }}</span>
</template>
<style scoped>
.local-image { max-width: 100%; max-height: 160px; object-fit: contain; border-radius: 6px; }
</style>
