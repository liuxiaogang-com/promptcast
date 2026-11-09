<!-- DataPanel.vue — 导入 / 导出 JSON。 -->
<script lang="ts">
import { defineComponent } from "vue";
import { save } from "@tauri-apps/plugin-dialog";
import { dataExportToFile, dataImportJson } from "../../api/data";
import { useUIStore } from "../../stores/ui";
import { usePromptsStore } from "../../stores/prompts";
import { useFoldersStore } from "../../stores/folders";
import { useTagsStore } from "../../stores/tags";

export default defineComponent({
  name: "DataPanel",
  data() {
    return {
      mode: "merge" as "merge" | "replace",
      busy: false,
    };
  },
  methods: {
    async exportJson() {
      // 旧实现用 <a download> blob 下载，Tauri webview 不支持，点了没反应。
      // 改用原生保存对话框选位置，再由后端写文件。
      let path: string | null = null;
      try {
        path = await save({
          defaultPath: `promptcast-backup-${Date.now()}.json`,
          filters: [{ name: "JSON", extensions: ["json"] }],
        });
      } catch (e) {
        useUIStore().pushToast(`打开保存对话框失败: ${e}`, "danger");
        return;
      }
      if (!path) return; // 用户取消
      this.busy = true;
      try {
        await dataExportToFile(path);
        useUIStore().pushToast("已导出 JSON", "success");
      } catch (e) {
        useUIStore().pushToast(`导出失败: ${e}`, "danger");
      } finally {
        this.busy = false;
      }
    },
    pickFile() {
      (this.$refs.file as HTMLInputElement).click();
    },
    async onFile(e: Event) {
      const f = (e.target as HTMLInputElement).files?.[0];
      if (!f) return;
      const json = await f.text();
      this.busy = true;
      try {
        const r = await dataImportJson(json, this.mode);
        useUIStore().pushToast(`导入完成: ${r.inserted} 条`, "success");
        await Promise.all([
          usePromptsStore().loadAll(),
          useFoldersStore().loadAll(),
          useTagsStore().loadAll(),
        ]);
      } catch (err) {
        useUIStore().pushToast(`导入失败: ${err}`, "danger");
      } finally {
        this.busy = false;
        (this.$refs.file as HTMLInputElement).value = "";
      }
    },
  },
});
</script>

<template>
  <section class="panel">
    <h3>数据</h3>
    <div class="card">
      <div class="row">
        <div>
          <div class="title">导出全部数据</div>
          <div class="sub">JSON 文件，含提示词、分类、标签、设置（不含 favicon 二进制）。</div>
        </div>
        <button class="primary" :disabled="busy" @click="exportJson">导出</button>
      </div>
    </div>
    <div class="card">
      <div class="row">
        <div>
          <div class="title">从 JSON 导入</div>
          <div class="sub">
            合并：保留现有数据，跳过 ID 冲突；
            替换：清空后再导入。
          </div>
        </div>
      </div>
      <div class="row">
        <label class="radio">
          <input type="radio" v-model="mode" value="merge" />
          合并
        </label>
        <label class="radio">
          <input type="radio" v-model="mode" value="replace" />
          替换（危险）
        </label>
        <span class="spacer" />
        <button class="primary" :disabled="busy" @click="pickFile">
          {{ busy ? "导入中…" : "选择文件…" }}
        </button>
      </div>
      <input ref="file" type="file" accept="application/json" hidden @change="onFile" />
    </div>
  </section>
</template>

<style scoped>
.panel { display: flex; flex-direction: column; gap: 12px; }
h3 { font-size: var(--fs-14); font-weight: 600; }
.card {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px;
  background: var(--bg-base);
  display: flex; flex-direction: column; gap: 12px;
}
.row {
  display: flex; flex-wrap: wrap; align-items: center; gap: 10px;
}
.title { font-size: var(--fs-13); font-weight: 500; }
.sub { font-size: var(--fs-11); color: var(--text-secondary); margin-top: 2px; line-height: 1.5; }
.primary {
  min-height: calc(30px * var(--font-scale));
  padding: 0 14px;
  border-radius: 6px;
  background: var(--accent);
  color: var(--accent-fg);
  font-size: var(--fs-12);
  font-weight: 600;
  flex-shrink: 0;
}
.primary:disabled { opacity: 0.5; }
.radio { display: inline-flex; align-items: center; gap: 4px; font-size: var(--fs-12); }
.spacer { flex: 1; }
</style>
