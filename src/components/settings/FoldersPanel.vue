<!-- FoldersPanel.vue — 文件夹管理（拖拽排序 + 重命名 + 删除）。 -->
<script lang="ts">
import { defineComponent } from "vue";
import { VueDraggable } from "vue-draggable-plus";
import { GripVertical, Pencil, Trash2 } from "lucide-vue-next";
import { useFoldersStore } from "../../stores/folders";
import { confirmDanger } from "../../utils/dialog";

export default defineComponent({
  name: "FoldersPanel",
  components: { VueDraggable, GripVertical, Pencil, Trash2 },
  data() {
    return {
      newName: "",
      editingId: null as number | null,
      editingValue: "",
    };
  },
  computed: {
    folders() { return useFoldersStore(); },
    list() { return this.folders.list; },
  },
  async mounted() { await this.folders.loadAll(); },
  methods: {
    async createNew() {
      const n = this.newName.trim();
      if (!n) return;
      await this.folders.create(n);
      this.newName = "";
    },
    startEdit(id: number, name: string) {
      this.editingId = id;
      this.editingValue = name;
    },
    async commitEdit(id: number) {
      // Enter 会先清空 editingId、移除 input，从而合成一次 blur 再调本方法；
      // 这里先判重再同步清状态，避免重复 rename + 重复 reload。
      if (this.editingId !== id) return;
      const v = this.editingValue.trim();
      this.editingId = null;
      if (v) await this.folders.rename(id, v);
    },
    async remove(id: number) {
      if (!(await confirmDanger("删除这个分类？里面的提示词会变为未分类。", "删除分类"))) return;
      await this.folders.remove(id);
    },
    async onDragEnd() {
      await this.folders.reorder(this.list.map((f) => f.id));
    },
  },
});
</script>

<template>
  <section class="panel">
    <header class="head">
      <div class="hl">
        <h3>分类管理</h3>
        <p class="sub">拖动手柄重排序；删除时其下的提示词会变成「未分类」。</p>
      </div>
      <form class="add-form" @submit.prevent="createNew">
        <input v-model="newName" placeholder="新分类名称" />
        <button type="submit" class="primary">添加</button>
      </form>
    </header>
    <VueDraggable
      :model-value="list"
      @update:model-value="(v: typeof list) => folders.list = v"
      handle=".grip"
      :animation="180"
      class="rows"
      @end="onDragEnd"
    >
      <div v-for="f in list" :key="f.id" class="row">
        <GripVertical :size="14" class="grip" />
        <input
          v-if="editingId === f.id"
          v-model="editingValue"
          @keydown.enter="commitEdit(f.id)"
          @blur="commitEdit(f.id)"
          autofocus
        />
        <span v-else class="name" @dblclick="startEdit(f.id, f.name)">{{ f.name }}</span>
        <span class="spacer" />
        <button class="ico" @click="startEdit(f.id, f.name)" title="重命名">
          <Pencil :size="13" />
        </button>
        <button class="ico danger" @click="remove(f.id)" title="删除">
          <Trash2 :size="13" />
        </button>
      </div>
      <p v-if="!list.length" class="empty">还没有分类</p>
    </VueDraggable>
  </section>
</template>

<style scoped>
.panel { display: flex; flex-direction: column; gap: 14px; }
.head { display: flex; flex-wrap: wrap; gap: 12px; }
.hl { flex: 1; }
.head h3 { font-size: var(--fs-14); font-weight: 600; }
.sub { font-size: var(--fs-11); color: var(--text-secondary); margin-top: 4px; line-height: 1.5; }
.add-form { display: flex; flex-wrap: wrap; gap: 6px; min-width: 0; }
.add-form input {
  width: 140px;
  min-height: calc(30px * var(--font-scale));
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg-input);
  font-size: var(--fs-12);
  outline: none;
  color: var(--text-primary);
}
.primary {
  min-height: calc(30px * var(--font-scale));
  padding: 0 12px;
  border-radius: 6px;
  background: var(--accent);
  color: var(--accent-fg);
  font-size: var(--fs-12);
  font-weight: 600;
}
.rows { display: flex; flex-direction: column; gap: 6px; }
.row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  min-height: calc(40px * var(--font-scale));
  padding: 0 12px;
  background: var(--bg-base);
  border: 1px solid var(--border);
  border-radius: 8px;
}
.grip { cursor: grab; color: var(--text-tertiary); flex-shrink: 0; }
.name { overflow-wrap: anywhere; min-width: 0; font-size: var(--fs-13); }
.spacer { flex: 1; }
.ico {
  width: 24px; min-height: calc(24px * var(--font-scale));
  display: flex; align-items: center; justify-content: center;
  border-radius: 5px;
  font-size: var(--fs-12);
  color: var(--text-secondary);
}
.ico:hover { background: var(--bg-hover); }
.ico.danger { color: var(--danger); }
.empty { font-size: var(--fs-12); color: var(--text-tertiary); padding: 16px; text-align: center; }
</style>
