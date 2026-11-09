import { describe, it, expect, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { updateChangelog } from "../api/update";
import ReleaseNotes from "./ReleaseNotes.vue";

vi.mock("../api/update", () => ({ updateChangelog: vi.fn() }));

describe("ReleaseNotes", () => {
  it("将发布时间转换为本地时间，不显示 ISO 时间串", async () => {
    const localDate = new Date(2026, 10, 9, 16, 12, 31);
    vi.mocked(updateChangelog).mockResolvedValueOnce([
      { version: "0.0.10", title: "新版", notes: "日志", pub_date: localDate.toISOString() },
    ]);
    const wrapper = mount(ReleaseNotes);
    await flushPromises();
    expect(wrapper.text()).toContain("2026-11-09 16:12");
    expect(wrapper.text()).not.toContain(localDate.toISOString());
  });
  it("旧日志只显示原日期，无效日期显示占位提示", async () => {
    vi.mocked(updateChangelog).mockResolvedValueOnce([
      { version: "0.0.8", title: "旧版", notes: "日志", pub_date: "2026-06-25" },
      { version: "0.0.7", title: "旧版", notes: "日志", pub_date: "invalid" },
    ]);
    const wrapper = mount(ReleaseNotes);
    await flushPromises();
    expect(wrapper.findAll(".entry .hint").map((el) => el.text())).toEqual(["2026-06-25", "日期未知"]);
  });
  it("独立展示历史日志，并按指定版本筛选", async () => {
    vi.mocked(updateChangelog).mockResolvedValue([
      { version: "0.0.9", title: "新版", notes: "新日志", pub_date: null },
      { version: "0.0.8", title: "旧版", notes: "旧日志", pub_date: null },
    ]);
    const wrapper = mount(ReleaseNotes);
    await flushPromises();
    expect(wrapper.text()).toContain("旧日志");
    await wrapper.setProps({ version: "v0.0.9" });
    expect(wrapper.text()).toContain("新日志");
    expect(wrapper.text()).not.toContain("旧日志");
  });
  it("日志请求失败时可以单独重试", async () => {
    vi.mocked(updateChangelog).mockRejectedValueOnce(new Error("offline"));
    const wrapper = mount(ReleaseNotes);
    await flushPromises();
    expect(wrapper.text()).toContain("不影响检查和安装更新");
    vi.mocked(updateChangelog).mockResolvedValueOnce([]);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("暂无该版本的日志");
  });
});
