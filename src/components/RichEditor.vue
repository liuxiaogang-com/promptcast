<script lang="ts">
import { defineComponent, markRaw, shallowRef, type PropType } from "vue";
import { Editor, EditorContent, type JSONContent } from "@tiptap/vue-3";
import StarterKit from "@tiptap/starter-kit";
import Image from "@tiptap/extension-image";
import { imageGet, importImageFile } from "../api/media";

const LocalImage = Image.extend({
  addNodeView() {
    return ({ node }) => {
      const dom = document.createElement("img");
      dom.alt = "加载图片…";
      const id = String(node.attrs.src).replace(/^promptcast-image:/, "");
      let disposed = false;
      void imageGet(id).then((asset) => { if (!disposed) { dom.src = asset.data_url; dom.alt = "图片"; } })
        .catch(() => { if (!disposed) dom.alt = "图片暂不可用"; });
      return { dom, destroy() { disposed = true; } };
    };
  },
});

export default defineComponent({
  components: { EditorContent },
  props: {
    modelValue: { type: Object as PropType<JSONContent>, required: true },
    readonly: { type: Boolean, default: false },
  },
  emits: ["update:modelValue", "text", "busy"],
  setup() { return { editor: shallowRef<Editor>() }; },
  data() { return { busy: false, error: "" }; },
  watch: {
    modelValue: {
      deep: true,
      handler(value: JSONContent) {
        if (this.editor && JSON.stringify(this.editor.getJSON()) !== JSON.stringify(value)) {
          this.editor.commands.setContent(value, { emitUpdate: false });
        }
      },
    },
  },
  mounted() {
    this.editor = markRaw(new Editor({
      extensions: [StarterKit.configure({ link: false, underline: false }), LocalImage],
      content: this.modelValue,
      editable: !this.readonly,
      onUpdate: ({ editor }) => { this.$emit("update:modelValue", editor.getJSON()); this.$emit("text", editor.getText()); },
      editorProps: {
        attributes: { "aria-label": "图文正文" },
        handlePaste: (_view, event) => {
          const files = Array.from(event.clipboardData?.files ?? []).filter((f) => f.type.startsWith("image/"));
          if (!files.length) return false;
          event.preventDefault(); void this.insertImages(files); return true;
        },
        // 外部图片必须经本地入库，不保留远程地址或未知 data URL。
        transformPastedHTML: (html) => {
          const doc = new DOMParser().parseFromString(html, "text/html");
          const images = doc.querySelectorAll("img");
          if (images.length) this.error = "外部网页图片未保存，请复制图片后粘贴，或使用“插入图片”。";
          images.forEach((img) => img.remove());
          return doc.body.innerHTML;
        },
      },
    }));
  },
  beforeUnmount() { this.editor?.destroy(); },
  methods: {
    async insertImages(files: File[]) {
      if (!this.editor || this.readonly || this.busy) return;
      this.busy = true; this.error = ""; this.$emit("busy", true);
      try {
        for (const file of files) {
          const id = await importImageFile(file);
          if (this.editor.isDestroyed) return;
          this.editor.chain().focus().setImage({ src: `promptcast-image:${id}` }).run();
        }
      } catch (e) { this.error = String(e); }
      finally { this.busy = false; this.$emit("busy", false); }
    },
    pick(event: Event) {
      const input = event.target as HTMLInputElement;
      void this.insertImages(Array.from(input.files ?? [])); input.value = "";
    },
  },
});
</script>
<template>
  <div class="rich" :class="{ readonly }">
    <div v-if="editor && !readonly" class="toolbar">
      <button type="button" :aria-pressed="editor.isActive('bold')" @click="editor.chain().focus().toggleBold().run()">粗体</button>
      <button type="button" :aria-pressed="editor.isActive('italic')" @click="editor.chain().focus().toggleItalic().run()">斜体</button>
      <button type="button" @click="editor.chain().focus().toggleBulletList().run()">列表</button>
      <button type="button" :disabled="busy" @click="($refs.file as HTMLInputElement).click()">{{ busy ? "保存图片…" : "插入图片" }}</button>
      <input ref="file" type="file" accept="image/png,image/jpeg,image/webp" multiple hidden @change="pick" />
    </div>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <EditorContent :editor="editor" />
  </div>
</template>
<style scoped>
.rich { border: 1px solid var(--border); border-radius: 8px; background: var(--bg-input); min-height: 180px; }
.readonly { border: 0; background: transparent; min-height: 0; }
.toolbar { display: flex; flex-wrap: wrap; gap: 6px; padding: 8px; border-bottom: 1px solid var(--border); }
.toolbar button { padding: 4px 8px; border-radius: 5px; font-size: var(--fs-12); }
.toolbar button:hover, .toolbar button[aria-pressed="true"] { background: var(--bg-selected); }
.error { padding: 8px; color: var(--danger); font-size: var(--fs-12); }
.rich :deep(.tiptap) { padding: 12px; min-height: 160px; outline: none; font-size: var(--fs-13); line-height: 1.7; overflow-wrap: anywhere; user-select: text; }
.readonly :deep(.tiptap) { padding: 0; min-height: 0; }
.rich :deep(p) { margin: 6px 0; }
.rich :deep(img) { max-width: 100%; height: auto; margin: 8px 0; }
.rich :deep(ul) { list-style: disc; padding-left: 24px; }
.rich :deep(ol) { list-style: decimal; padding-left: 24px; }
.rich :deep(pre) { white-space: pre-wrap; background: var(--bg-base); padding: 8px; }
.rich :deep(.ProseMirror-selectednode) { outline: 2px solid var(--accent); }
</style>
