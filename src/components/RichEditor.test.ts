import { it, expect, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import RichEditor from "./RichEditor.vue";

vi.mock("../api/media", () => ({
  imageGet: vi.fn().mockResolvedValue({ data_url: "data:image/png;base64,AA==" }),
  importImageFile: vi.fn(),
}));

it("图文加载显示本地图片，文档保留图片引用并保留文字格式", async () => {
  const id = "a".repeat(64);
  const document = { type: "doc", content: [
    { type: "paragraph", content: [{ type: "text", text: "图文正文", marks: [{ type: "bold" }] }] },
    { type: "image", attrs: { src: `promptcast-image:${id}` } },
  ] };
  const wrapper = mount(RichEditor, { props: { modelValue: document } });
  await flushPromises();
  expect(wrapper.get("strong").text()).toBe("图文正文");
  expect(wrapper.get("img").attributes("src")).toBe("data:image/png;base64,AA==");
  const saved = wrapper.vm.editor?.getJSON();
  expect(saved?.content?.[1].attrs?.src).toBe(`promptcast-image:${id}`);
  expect(JSON.stringify(saved)).not.toContain("base64");
  wrapper.unmount();
});

it("外部更新刷新图文预览，不触发编辑回写", async () => {
  const doc = (text: string) => ({ type: "doc", content: [{ type: "paragraph", content: [{ type: "text", text }] }] });
  const wrapper = mount(RichEditor, { props: { modelValue: doc("旧正文"), readonly: true } });
  await flushPromises();
  await wrapper.setProps({ modelValue: doc("新正文") });
  expect(wrapper.get(".tiptap").text()).toBe("新正文");
  expect(wrapper.emitted("update:modelValue")).toBeUndefined();
  wrapper.unmount();
});
