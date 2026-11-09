<!-- BaseToast.vue — 顶部 toast 队列。 -->
<script lang="ts">
import { defineComponent } from "vue";
import { useUIStore } from "../../stores/ui";

export default defineComponent({
  name: "BaseToast",
  computed: {
    toasts() {
      return useUIStore().toasts;
    },
  },
  methods: {
    dismiss(id: number) {
      useUIStore().dismissToast(id);
    },
  },
});
</script>

<template>
  <div class="toasts">
    <transition-group name="toast">
      <div
        v-for="t in toasts"
        :key="t.id"
        class="toast"
        :class="`toast-${t.kind}`"
        @click="dismiss(t.id)"
      >
        {{ t.text }}
      </div>
    </transition-group>
  </div>
</template>

<style scoped>
.toasts {
  position: fixed;
  top: 12px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 1000;
  display: flex;
  flex-direction: column;
  gap: 6px;
  pointer-events: none;
}
.toast {
  pointer-events: auto;
  background: var(--bg-surface);
  color: var(--text-primary);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px 14px;
  font-size: var(--fs-12);
  box-shadow: var(--shadow-md);
  max-width: 320px;
  cursor: pointer;
}
.toast-success { border-color: var(--success); }
.toast-warning { border-color: var(--warning); }
.toast-danger { border-color: var(--danger); color: var(--danger); }

.toast-enter-active, .toast-leave-active {
  transition: all var(--dur-base) var(--ease-out);
}
.toast-enter-from, .toast-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
</style>
