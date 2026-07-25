// RoutingSection 回归：model 为空或含空白字符不落盘（行内提示，configSetRole 不下发）；
// fallback 配出 a<->b 互指降级时行内出循环提示。
import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  cfg: vi.fn(async () => ({ roles: {} }) as unknown),
  setRole: vi.fn(async (_r: string, _p: string, _m: string, _f?: string, _a?: string) => {}),
  stats: vi.fn(async () => ({ describe: "", history: [] })),
  accounts: vi.fn(async () => []),
  list: vi.fn(async () => []),
  catalog: vi.fn(async () => []),
  dispatch: vi.fn(async () => ({ role: "chat", provider: "p", model: "m", answer: "pong" })),
}));

vi.mock("../../lib/chat", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/chat")>();
  return { ...orig, configGet: h.cfg, configSetRole: h.setRole };
});

vi.mock("../../lib/provider", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/provider")>();
  return {
    ...orig,
    mrmStats: h.stats,
    providerAccounts: h.accounts,
    providerList: h.list,
    testDispatch: h.dispatch,
  };
});

vi.mock("../../lib/models", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/models")>();
  return { ...orig, modelsCatalog: h.catalog };
});

import RoutingSection from "./RoutingSection";

afterEach(() => {
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

beforeEach(() => {
  h.stats.mockResolvedValue({ describe: "", history: [] });
  h.accounts.mockResolvedValue([]);
  h.list.mockResolvedValue([]);
  h.catalog.mockResolvedValue([]);
});

function modelInput(role: string): HTMLInputElement {
  const found = document.body.querySelector<HTMLInputElement>(`input[list="models-${role}"]`);
  if (!found) throw new Error(`model input not found: ${role}`);
  return found;
}

describe("RoutingSection model 校验", () => {
  it("含空白字符的 model 不落盘，行内提示未保存", async () => {
    h.cfg.mockResolvedValue({
      roles: { chat: { provider: "anthropic", model: "claude-sonnet-4-6" } },
    });
    const dispose = render(() => <RoutingSection />, document.body);
    await vi.waitFor(() => expect(modelInput("chat").value).toBe("claude-sonnet-4-6"));

    const input = modelInput("chat");
    input.value = "bad model";
    input.dispatchEvent(new Event("change", { bubbles: true }));
    await vi.waitFor(() => expect(document.body.textContent).toContain("未保存"));
    expect(h.setRole).not.toHaveBeenCalled();
    // 本地态仍回显非法值（受控输入不被吞），用户可继续修正
    expect(modelInput("chat").value).toBe("bad model");

    input.value = "claude-opus-4-7";
    input.dispatchEvent(new Event("change", { bubbles: true }));
    await vi.waitFor(() =>
      expect(h.setRole).toHaveBeenCalledWith("chat", "anthropic", "claude-opus-4-7", "", ""),
    );
    dispose();
  });

  it("缺省空 model 未编辑不出提示（不吵）", async () => {
    h.cfg.mockResolvedValue({ roles: {} });
    const dispose = render(() => <RoutingSection />, document.body);
    await new Promise((r) => setTimeout(r, 20));
    expect(document.body.textContent).not.toContain("未保存");
    dispose();
  });
});

describe("RoutingSection fallback 循环提示", () => {
  it("a<->b 互指降级：两行都出提示", async () => {
    h.cfg.mockResolvedValue({
      roles: {
        chat: { provider: "anthropic", model: "m1", fallback: "execution" },
        execution: { provider: "anthropic", model: "m2", fallback: "chat" },
      },
    });
    const dispose = render(() => <RoutingSection />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("互指降级"));
    const hints = [...document.body.querySelectorAll("span")].filter((s) =>
      s.textContent?.includes("互指降级"),
    );
    expect(hints.length).toBe(2);
    dispose();
  });
});
