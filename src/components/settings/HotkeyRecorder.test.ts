import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import HotkeyRecorder from "./HotkeyRecorder.vue";

describe("HotkeyRecorder", () => {
  it("点击录制并提交组合键，清除按钮提交空值", async () => {
    const wrapper = mount(HotkeyRecorder, { props: { modelValue: "CmdOrCtrl+Shift+P" } });
    await wrapper.get(".combo").trigger("click");
    expect(wrapper.text()).toContain("按下组合键");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true, shiftKey: true }));
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual(["CmdOrCtrl+Shift+K"]);
    await wrapper.get(".clear").trigger("click");
    expect(wrapper.emitted("update:modelValue")?.[1]).toEqual([""]);
    wrapper.unmount();
  });

  it("取消、失焦和卸载不会提交，保存期间不能重新录制或清除", async () => {
    const wrapper = mount(HotkeyRecorder, { props: { modelValue: "CmdOrCtrl+Shift+P" } });
    await wrapper.get(".combo").trigger("click");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await wrapper.get(".combo").trigger("click");
    await wrapper.get(".combo").trigger("blur");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true }));
    await wrapper.setProps({ disabled: true });
    expect(wrapper.get(".combo").attributes("disabled")).toBeDefined();
    expect(wrapper.get(".clear").attributes("disabled")).toBeDefined();
    await wrapper.setProps({ disabled: false });
    await wrapper.get(".combo").trigger("click");
    wrapper.unmount();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true }));
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
  });
});
