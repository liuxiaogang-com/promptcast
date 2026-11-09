import { describe, it, expect, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { updateChangelog } from "../api/update";
import ReleaseNotes from "./ReleaseNotes.vue";

vi.mock("../api/update", () => ({ updateChangelog: vi.fn() }));

describe("ReleaseNotes", () => {
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
