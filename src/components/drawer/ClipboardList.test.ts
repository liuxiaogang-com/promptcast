import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import ClipboardList from "./ClipboardList.vue";
import { imageGet } from "../../api/media";
import type { ClipEntry } from "../../types/clipboard";

vi.mock("../../api/media", () => ({ imageGet: vi.fn().mockResolvedValue({ data_url: "data:image/png;base64,AA==" }) }));
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
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

it("文字行使用实际内容高度，时间下面不保留固定行高空白", async () => {
  const wrapper = mount(ClipboardList, { props: { entries: entries(), selectedId: 500 } });
  await flushPromises();
  const shortRow = wrapper.get('[data-id="500"]');
  const longRow = wrapper.get('[data-id="499"]');
  expect((shortRow.element as HTMLElement).style.height).toBe("");
  vi.spyOn(shortRow.element, "getBoundingClientRect").mockReturnValue({ height: 54 } as DOMRect);
  vi.spyOn(longRow.element, "getBoundingClientRect").mockReturnValue({ height: 72 } as DOMRect);
  wrapper.vm.measureRows();
  await flushPromises();
  expect(wrapper.vm.rows[0].height).toBe(54);
  expect(wrapper.vm.rows[1].top).toBe(54);
  expect(wrapper.vm.rows[1].height).toBe(72);
  expect(wrapper.vm.rows[2].top).toBe(126);
  wrapper.unmount();
});

it("图片行没有固定高度，缩略图加载后的实际高度用于排列下一条", async () => {
  const wrapper = mount(ClipboardList, { props: { entries: entries(true) } });
  await flushPromises();
  const row = wrapper.get('[data-id="500"]').element as HTMLElement;
  expect(row.style.height).toBe("");
  const geometry = vi.spyOn(row, "getBoundingClientRect").mockReturnValue({ height: 54 } as DOMRect);
  wrapper.vm.measureRows();
  await flushPromises();
  expect(wrapper.vm.rows[1].top).toBe(54);
  geometry.mockReturnValue({ height: 168 } as DOMRect);
  wrapper.vm.measure();
  await flushPromises();
  expect(wrapper.vm.rows[1].top).toBe(168);
  expect(wrapper.vm.observedRows.has(row)).toBe(true);
  wrapper.unmount();
});

it("图片加载中和失败时也按提示内容高度排列，不保留图片占位空白", async () => {
  let fail!: (reason: Error) => void;
  vi.mocked(imageGet).mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject; }));
  const wrapper = mount(ClipboardList, { props: { entries: entries(true).slice(0, 2) } });
  await flushPromises();
  const row = wrapper.get('[data-id="500"]');
  expect(row.text()).toContain("加载图片…");
  expect((row.element as HTMLElement).style.height).toBe("");
  vi.spyOn(row.element, "getBoundingClientRect").mockReturnValue({ height: 54 } as DOMRect);
  wrapper.vm.measureRows();
  expect(wrapper.vm.rows[1].top).toBe(54);
  fail(new Error("图片不存在"));
  await flushPromises();
  expect(row.text()).toContain("图片暂不可用");
  wrapper.vm.measureRows();
  expect(wrapper.vm.rows[1].top).toBe(54);
  wrapper.unmount();
});
