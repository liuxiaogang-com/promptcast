import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import ClipboardList from "./ClipboardList.vue";
import { imageGet } from "../../api/media";
import type { ClipEntry } from "../../types/clipboard";

vi.mock("../../api/media", () => ({ imageGet: vi.fn().mockResolvedValue({ data_url: "data:image/png;base64,AA==" }) }));
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
});
afterEach(() => vi.unstubAllGlobals());

function entries(images = false): ClipEntry[] {
  return Array.from({ length: 500 }, (_, index) => ({
    id: 500 - index, content: `内容 ${index}`, char_count: 5, created_at: Date.now(),
    image_id: images ? `image-${index}` : null,
  }));
}

it("大量图片历史只挂载可视区域附近的条目，键盘可定位未渲染的历史", async () => {
  const wrapper = mount(ClipboardList, { props: { entries: entries(true), selectedId: 500 } });
  await flushPromises();
  expect(wrapper.findAll("[data-id]").length).toBeLessThan(20);
  expect(vi.mocked(imageGet).mock.calls.length).toBeLessThan(20);
  expect(wrapper.find('[data-id="1"]').exists()).toBe(false);
  await wrapper.setProps({ selectedId: 1 });
  await flushPromises();
  expect(wrapper.find('[data-id="1"]').exists()).toBe(true);
  expect(wrapper.findAll("[data-id]").length).toBeLessThan(20);
  expect((wrapper.get(".viewport").element as HTMLElement).scrollTop).toBeGreaterThan(0);
  wrapper.unmount();
});

it("新记录和再次唤起回到顶部，普通刷新保留位置与勾选，大字号重新计算高度", async () => {
  const list = entries();
  const wrapper = mount(ClipboardList, { props: { entries: list, selectedId: 500 } });
  await flushPromises();
  await wrapper.get('[data-id="500"] .item').trigger("click");
  await wrapper.setProps({ selectedId: 1 });
  await flushPromises();
  const viewport = wrapper.get(".viewport").element as HTMLElement;
  const oldTop = viewport.scrollTop;
  await wrapper.setProps({ entries: [...list] });
  expect(viewport.scrollTop).toBe(oldTop);
  expect(wrapper.text()).toContain("已选 1");
  await wrapper.setProps({ entries: [{ ...list[0], id: 501 }, ...list], selectedId: 501 });
  await flushPromises();
  expect(viewport.scrollTop).toBe(0);
  expect(wrapper.find('[data-id="501"]').exists()).toBe(true);
  await wrapper.setProps({ selectedId: 1 });
  await flushPromises();
  await wrapper.setProps({ resetToken: 1, selectedId: 501 });
  await flushPromises();
  expect(viewport.scrollTop).toBe(0);
  vi.spyOn(wrapper.vm.$refs.measure as HTMLElement, "getBoundingClientRect").mockReturnValue({ height: 120 } as DOMRect);
  wrapper.vm.measure();
  await flushPromises();
  expect(wrapper.vm.totalHeight).toBeCloseTo(501 * 88 * 1.2);
  expect(wrapper.findAll("[data-id]").length).toBeLessThan(30);
  wrapper.unmount();
});
