<!-- SitesPanel.vue — 网址管理（拖拽排序 + 增删改 + 重新抓 favicon）。 -->
<script lang="ts">
import { defineComponent } from "vue";
import { VueDraggable } from "vue-draggable-plus";
import { GripVertical, Pencil, Trash2, RefreshCw, Check, X } from "lucide-vue-next";
import { useSitesStore } from "../../stores/sites";
import { confirmDanger } from "../../utils/dialog";

export default defineComponent({
  name: "SitesPanel",
  components: { VueDraggable, GripVertical, Pencil, Trash2, RefreshCw, Check, X },
  data() {
    return {
      adding: false,
      newName: "",
      newUrl: "",
      editingId: null as number | null,
      eName: "",
      eUrl: "",
      busy: false,
    };
  },
  computed: {
    sites() { return useSitesStore(); },
    list() { return this.sites.list; },
  },
  async mounted() { await this.sites.loadAll(); },
  methods: {
    initial(name: string): string {
      return name.trim().charAt(0).toUpperCase() || "?";
    },
    async create() {
      const n = this.newName.trim();
      const u = this.newUrl.trim();
      if (!n || !u) return;
      this.busy = true;
      try { await this.sites.create(n, u); }
      finally { this.busy = false; }
      this.newName = ""; this.newUrl = ""; this.adding = false;
    },
    startEdit(id: number, name: string, url: string) {
      this.editingId = id;
      this.eName = name; this.eUrl = url;
    },
    async saveEdit() {
      if (this.editingId == null) return;
      await this.sites.update(this.editingId, this.eName.trim(), this.eUrl.trim());
      this.editingId = null;
    },
    async remove(id: number) {
      if (!(await confirmDanger("删除这个网址？", "删除网址"))) return;
      await this.sites.remove(id);
    },
    async refresh(id: number) {
      this.busy = true;
      try { await this.sites.refreshFavicon(id); }
      finally { this.busy = false; }
    },
    async onDragEnd() {
      await this.sites.reorder(this.list.map((s) => s.id));
    },
  },
});
</script>

<template>
  <section class="panel">
    <header class="head">
      <div class="hl">
        <h3>网址快捷</h3>
        <p class="sub">在抽屉底部显示一行 favicon，点击直接在浏览器打开。</p>
      </div>
      <button v-if="!adding" class="primary" @click="adding = true">+ 添加网址</button>
    </header>

    <form v-if="adding" class="add-form" @submit.prevent="create">
      <input v-model="newName" placeholder="名称（如 ChatGPT）" />
      <input v-model="newUrl" placeholder="https://…" />
      <button type="button" class="ghost" @click="adding = false">取消</button>
      <button type="submit" class="primary" :disabled="busy">
        {{ busy ? "抓取 favicon…" : "保存" }}
      </button>
    </form>

    <VueDraggable
      :model-value="list"
      @update:model-value="(v: typeof list) => sites.list = v"
      handle=".grip"
      :animation="180"
      class="rows"
      @end="onDragEnd"
    >
      <div v-for="s in list" :key="s.id" class="row">
        <GripVertical :size="14" class="grip" />
        <div class="ico-box">
          <img v-if="s.favicon_data_uri" :src="s.favicon_data_uri" alt="" />
          <span v-else class="ph">{{ initial(s.name) }}</span>
        </div>
        <template v-if="editingId === s.id">
          <input v-model="eName" class="edit-name" />
          <input v-model="eUrl" class="edit-url" />
          <button class="ico" @click="saveEdit"><Check :size="13" /></button>
          <button class="ico" @click="editingId = null"><X :size="13" /></button>
        </template>
        <template v-else>
          <div class="meta">
            <div class="name">{{ s.name }}</div>
            <div class="url">{{ s.url }}</div>
          </div>
          <span class="spacer" />
          <button class="ico" @click="refresh(s.id)" title="重新抓取图标">
            <RefreshCw :size="12" />
          </button>
          <button class="ico" @click="startEdit(s.id, s.name, s.url)" title="编辑">
            <Pencil :size="13" />
          </button>
          <button class="ico danger" @click="remove(s.id)" title="删除">
            <Trash2 :size="13" />
          </button>
        </template>
      </div>
      <p v-if="!list.length" class="empty">还没有网址</p>
    </VueDraggable>
  </section>
</template>

<style scoped>
.panel { display: flex; flex-direction: column; gap: 14px; }
.head { display: flex; align-items: flex-start; gap: 12px; }
.hl { flex: 1; }
.head h3 { font-size: var(--fs-14); font-weight: 600; }
.sub { font-size: var(--fs-11); color: var(--text-secondary); margin-top: 4px; line-height: 1.5; }

.primary {
  min-height: calc(30px * var(--font-scale));
  padding: 0 12px;
  border-radius: 6px;
  background: var(--accent);
  color: var(--accent-fg);
  font-size: var(--fs-12);
  font-weight: 600;
  flex-shrink: 0;
}
.primary:disabled { opacity: 0.5; }
.ghost {
  min-height: calc(30px * var(--font-scale)); padding: 0 12px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  font-size: var(--fs-12);
  color: var(--text-secondary);
}
.add-form {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 140px), 1fr));
  gap: 6px;
  padding: 12px;
  border: 1px dashed var(--border-strong);
  border-radius: 8px;
}
.add-form input {
  min-height: calc(30px * var(--font-scale)); padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg-input);
  color: var(--text-primary);
  outline: none;
  font-size: var(--fs-12);
}

.rows { display: flex; flex-direction: column; gap: 6px; }
.row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  background: var(--bg-base);
  border: 1px solid var(--border);
  border-radius: 8px;
}
.grip { cursor: grab; color: var(--text-tertiary); flex-shrink: 0; }
.ico-box {
  width: 28px; min-height: calc(28px * var(--font-scale));
  border-radius: 6px;
  background: var(--bg-surface);
  display: flex; align-items: center; justify-content: center;
  overflow: hidden;
}
.ico-box img { width: 18px; height: 18px; object-fit: contain; }
.ico-box .ph { font-weight: 700; font-size: var(--fs-12); color: var(--text-secondary); }
.meta { min-width: 0; }
.name { overflow-wrap: anywhere; min-width: 0; font-size: var(--fs-13); font-weight: 500; }
.url {
  font-size: var(--fs-11);
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.edit-name, .edit-url {
  min-height: calc(28px * var(--font-scale));
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 0 10px;
  background: var(--bg-input);
  color: var(--text-primary);
  font-size: var(--fs-12);
  outline: none;
}
.edit-name { width: 120px; }
.edit-url { flex: 1; }
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
