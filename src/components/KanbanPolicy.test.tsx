// 授权表单数值校验回归：小数/非数/0/Infinity 必须显式拒绝——静默截断或 null 序列化
// 会被后端当成「不设上限/永不过期」，授权面反向扩大。留空 = 不设置字段。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";
import KanbanPolicy from "./KanbanPolicy";
import { flash } from "../lib/flash";
import type { KanbanPolicySpec } from "../lib/chat";

const flush = () => new Promise((r) => setTimeout(r, 0));

function setup() {
  const onSave = vi.fn();
  const dispose = render(
    () => <KanbanPolicy policy={null} acting={false} onSave={onSave} onClose={() => {}} />,
    document.body,
  );
  const allowlist = document.body.querySelector(
    "textarea[aria-label='授权命令前缀']",
  ) as HTMLTextAreaElement;
  const uses = document.body.querySelector(
    "input[aria-label='最大自动放行次数']",
  ) as HTMLInputElement;
  const mins = document.body.querySelector(
    "input[aria-label='授权时限分钟数']",
  ) as HTMLInputElement;
  const saveBtn = [...document.body.querySelectorAll("button")].find((el) =>
    el.textContent?.includes("保存授权"),
  )!;
  const type = (el: HTMLInputElement | HTMLTextAreaElement, value: string) => {
    el.value = value;
    el.dispatchEvent(new Event("input", { bubbles: true }));
  };
  // 前缀为空时保存按钮 disabled：用例聚焦数值校验，先保证按钮可点
  type(allowlist, "cargo test");
  return { dispose, onSave, uses, mins, saveBtn, type };
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("KanbanPolicy 数值校验", () => {
  it("小数/非数/0/Infinity 拒绝保存并 flash 错误", async () => {
    for (const bad of ["1.5", "abc", "0", "Infinity"]) {
      const { dispose, onSave, uses, saveBtn } = setup();
      // type=number 的 value setter 按 HTML 规范把非浮点串净化成 ""（"abc"/"Infinity" 经正常
      // 赋值到不了 save）：实例级 get/set 绕过净化，覆盖异常值直达 handler 的路径
      let stored = bad;
      Object.defineProperty(uses, "value", {
        get: () => stored,
        set: (v: string) => {
          stored = v;
        },
        configurable: true,
      });
      uses.dispatchEvent(new Event("input", { bubbles: true }));
      await flush();
      saveBtn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      await flush();
      expect(onSave, `输入 ${JSON.stringify(bad)} 不得保存`).not.toHaveBeenCalled();
      expect(flash.msgs().some((m) => m.kind === "err")).toBe(true);
      dispose();
    }
  });

  it("时限分钟与次数同规则", async () => {
    const { dispose, onSave, mins, saveBtn } = setup();
    let stored = "1.5";
    Object.defineProperty(mins, "value", {
      get: () => stored,
      set: (v: string) => {
        stored = v;
      },
      configurable: true,
    });
    mins.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();
    saveBtn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(onSave).not.toHaveBeenCalled();
    dispose();
  });

  it("留空 = 不设置字段", async () => {
    const { dispose, onSave, saveBtn } = setup();
    await flush();
    saveBtn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(onSave).toHaveBeenCalledWith({ allowlist: ["cargo test"] });
    dispose();
  });

  it("合法整数通过：次数直传，分钟换算 expires_at_ms", async () => {
    const { dispose, onSave, uses, mins, saveBtn, type } = setup();
    type(uses, "5");
    type(mins, "30");
    await flush();
    const before = Date.now();
    saveBtn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    const policy = onSave.mock.calls[0]![0] as KanbanPolicySpec;
    expect(policy.allowlist).toEqual(["cargo test"]);
    expect(policy.max_uses).toBe(5);
    expect(policy.expires_at_ms).toBeGreaterThanOrEqual(before + 30 * 60_000);
    expect(policy.expires_at_ms).toBeLessThanOrEqual(Date.now() + 30 * 60_000);
    dispose();
  });
});
