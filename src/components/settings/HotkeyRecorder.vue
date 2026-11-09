<!-- HotkeyRecorder.vue — 录制全局快捷键。 -->
<script lang="ts">
import { defineComponent } from "vue";
import { X } from "lucide-vue-next";
import { isMac } from "../../utils/format";

export default defineComponent({
  name: "HotkeyRecorder",
  components: { X },
  props: {
    disabled: { type: Boolean, default: false },
    modelValue: { type: String, default: "" },
  },
  emits: {
    "update:modelValue": (_v: string) => true,
  },
  data() {
    return { recording: false };
  },
  computed: {
    display(): string {
      if (this.recording) return "按下组合键…";
      if (this.disabled) return "保存中…";
      if (!this.modelValue) return "点击设置";
      return this.modelValue
        .replace(/CmdOrCtrl/g, isMac() ? "⌘" : "Ctrl")
        .replace(/Shift/g, isMac() ? "⇧" : "Shift")
        .replace(/Alt/g, isMac() ? "⌥" : "Alt")
        .replace(/Super/g, isMac() ? "⌘" : "Win")
        .replace(/Meta/g, isMac() ? "⌘" : "Win")
        .replace(/\+/g, " ");
    },
  },
  beforeUnmount() { this.cancel(); },
  methods: {
    start() {
      if (this.disabled) return;
      this.recording = true;
      document.addEventListener("keydown", this.onKey, true);
    },
    cancel() {
      this.recording = false;
      document.removeEventListener("keydown", this.onKey, true);
    },
    clear() {
      if (this.disabled) return;
      this.cancel();
      this.$emit("update:modelValue", "");
    },
    onKey(e: KeyboardEvent) {
      if (this.disabled) { this.cancel(); return; }
      if (e.key === "Tab") { this.cancel(); return; }
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") { this.cancel(); return; }
      // 区分 Win/Cmd 与 Ctrl：
      //   - macOS: metaKey = ⌘ → 用作 CmdOrCtrl（与 Win 上 Ctrl 互通）
      //   - Windows: metaKey = Win 键 → 单独标 Super；ctrlKey 才是 CmdOrCtrl
      const mac = isMac();
      const mods: string[] = [];
      if (mac) {
        if (e.metaKey || e.ctrlKey) mods.push("CmdOrCtrl");
      } else {
        if (e.ctrlKey) mods.push("CmdOrCtrl");
        if (e.metaKey) mods.push("Super");
      }
      if (e.shiftKey) mods.push("Shift");
      if (e.altKey) mods.push("Alt");
      if (["Control", "Meta", "Shift", "Alt"].includes(e.key)) return;
      // 必须至少一个 modifier，否则单键当快捷键太容易冲突。
      if (mods.length === 0) return;
      const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
      const combo = [...mods, key].join("+");
      this.$emit("update:modelValue", combo);
      this.cancel();
    },
  },
});
</script>

<template>
  <div class="hk" :class="{ recording }">
    <button class="combo" :disabled="disabled" :aria-pressed="recording"
      aria-label="录制全局唤起快捷键" :title="recording ? '按 Esc 取消' : '点击录制，按下组合键后自动保存'"
      @click="start" @blur="cancel">{{ display }}</button>
    <button v-if="modelValue" class="clear" :disabled="disabled" aria-label="清除快捷键"
      title="清除快捷键" @click="clear"><X :size="14" /></button>
  </div>
</template>

<style scoped>
.hk {
  display: inline-flex; align-items: center;
  border: 1px solid var(--border); border-radius: 6px;
  background: var(--bg-input);
}
.hk.recording, .hk:focus-within { border-color: var(--accent); }
.combo {
  min-width: 120px; min-height: calc(30px * var(--font-scale)); padding: 0 10px;
  text-align: left; font-family: var(--font-mono);
  font-size: var(--fs-12); font-weight: 600; color: var(--text-primary);
}
.clear {
  display: inline-flex; align-items: center; justify-content: center;
  width: 28px; min-height: calc(28px * var(--font-scale)); border-radius: 4px; margin-right: 2px;
  color: var(--text-secondary);
}
.clear:hover { background: var(--bg-hover); color: var(--danger); }
button:disabled { opacity: 0.6; cursor: wait; }
</style>
