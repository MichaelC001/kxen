// Settings 通用区回归：乐观更新失败时回滚，不留与后端不一致的假状态。
import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { JSX } from "solid-js";

const h = vi.hoisted(() => ({
  cfg: vi.fn(async () => ({ roles: {}, send_when_running: "queue" }) as unknown),
  rpc: vi.fn((_method: string, _params?: unknown) => Promise.resolve({}) as Promise<unknown>),
  resync: new Set<() => void>(),
}));

vi.mock("../lib/chat", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../lib/chat")>();
  return { ...orig, configGet: h.cfg };
});

vi.mock("../lib/client", () => ({
  client: {
    rpc: h.rpc,
    onResync: (cb: () => void) => {
      h.resync.add(cb);
      return () => h.resync.delete(cb);
    },
  },
}));

// <A> 依赖 Router 上下文：测试无路由装配，桩成普通锚
vi.mock("@solidjs/router", () => ({
  A: (props: { href: string; class?: string; children?: JSX.Element }) => (
    <a href={props.href} class={props.class}>
      {props.children}
    </a>
  ),
}));
vi.mock("../components/settings/ProvidersSection", () => ({
  default: () => <div>providers-content</div>,
}));

import Settings from "./Settings";
import { flash } from "../lib/flash";

const flush = () => new Promise((r) => setTimeout(r, 0));

function btnByText(text: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent === text,
  );
  if (!found) throw new Error(`button not found: ${text}`);
  return found;
}

function experimentToggles(text: "已启用" | "已关闭"): HTMLButtonElement[] {
  const heading = [...document.body.querySelectorAll<HTMLDivElement>("div")].find(
    (element) => element.textContent === "实验能力与数据边界",
  );
  if (!heading?.parentElement) throw new Error("experimental settings section not found");
  return [...heading.parentElement.querySelectorAll<HTMLButtonElement>("button")].filter(
    (button) => button.textContent === text,
  );
}

beforeEach(() => {
  h.cfg.mockReset();
  h.cfg.mockResolvedValue({ roles: {}, send_when_running: "queue" });
  h.rpc.mockReset();
  h.rpc.mockResolvedValue({});
  h.resync.clear();
});

afterEach(() => {
  document.body.innerHTML = "";
  for (const m of flash.msgs()) flash.dismiss(m.id);
  vi.clearAllMocks();
});

describe("Settings 运行中发送", () => {
  it("设置分类使用垂直页签语义和键盘导航", () => {
    const dispose = render(() => <Settings />, document.body);
    const general = btnByText("通用");
    general.focus();
    general.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }),
    );
    const providers = btnByText("提供商");
    expect(document.activeElement).toBe(providers);
    expect(providers.getAttribute("aria-selected")).toBe("true");
    expect(document.querySelector("[role='tabpanel']")?.getAttribute("aria-labelledby")).toBe(
      providers.id,
    );
    dispose();
  });

  it("配置读取失败：显示 UNKNOWN，禁止提交默认值", async () => {
    h.cfg.mockRejectedValue(new Error("config unavailable"));
    const dispose = render(() => <Settings />, document.body);

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain("配置读取失败，当前值为 UNKNOWN");
      expect(document.body.textContent).toContain("config unavailable");
    });
    expect(btnByText("排队").disabled).toBe(true);
    expect(btnByText("打断").disabled).toBe(true);
    expect(h.rpc).not.toHaveBeenCalledWith("config.set_send_policy", expect.anything());
    dispose();
  });

  it("RPC 失败：回滚到旧策略并 flashErr，不留假状态", async () => {
    h.rpc.mockImplementation((method: string) =>
      method === "config.set_send_policy"
        ? Promise.reject(new Error("disk read-only"))
        : Promise.resolve({}),
    );
    const dispose = render(() => <Settings />, document.body);
    await flush();

    btnByText("打断").click();
    await vi.waitFor(() => {
      expect(h.rpc).toHaveBeenCalledWith("config.set_send_policy", { policy: "interrupt" });
      const err = flash.msgs().find((m) => m.kind === "err");
      expect(err?.text).toContain("保存失败");
      expect(err?.text).toContain("disk read-only");
    });
    // 回滚：高亮回到「排队」
    await vi.waitFor(() => expect(btnByText("排队").className).toContain("border-[var(--accent)]"));
    expect(btnByText("打断").className).not.toContain("border-[var(--accent)]");
    dispose();
  });

  it("RPC 成功：切到打断且不报错", async () => {
    const dispose = render(() => <Settings />, document.body);
    await flush();
    btnByText("打断").click();
    await vi.waitFor(() => expect(btnByText("打断").className).toContain("border-[var(--accent)]"));
    expect(flash.msgs().some((m) => m.kind === "err")).toBe(false);
    dispose();
  });

  it("断线 resync 重拉 config/当前 Workspace MRM 概览，卸载后退订", async () => {
    const readinessCalls = () =>
      h.rpc.mock.calls.filter(([method]) => method === "mrm.readiness").length;
    const dispose = render(() => <Settings />, document.body);
    await vi.waitFor(() => expect(h.cfg).toHaveBeenCalledTimes(1));
    expect(readinessCalls()).toBe(1);
    expect(h.resync.size).toBe(1);

    h.resync.forEach((cb) => cb());
    await vi.waitFor(() => expect(h.cfg).toHaveBeenCalledTimes(2));
    expect(readinessCalls()).toBe(2);

    dispose();
    expect(h.resync.size).toBe(0);
  });
});

describe("Settings 首次运行检查", () => {
  it("LocalFree 无凭证时按当前 Workspace MRM 报告判定 PASS", async () => {
    h.rpc.mockImplementation((method: string) => {
      if (method === "mrm.readiness") {
        return Promise.resolve({
          workspace_ready: true,
          chat_ready: true,
          agents_ready: true,
          all_ready: true,
          roles: [
            { role: "chat", configured: true, ready: true, status: "ready", provider: "ollama" },
          ],
        });
      }
      return Promise.resolve({});
    });
    const dispose = render(() => <Settings />, document.body);
    await vi.waitFor(() => {
      const label = [...document.body.querySelectorAll("span")].find(
        (el) => el.textContent === "当前 Workspace 的 chat 路由可派发",
      );
      const row = label?.parentElement;
      expect(row?.textContent).toContain("PASS");
    });
    dispose();
  });

  it("任一 Agent 角色不可派发时显示具体角色和原因", async () => {
    h.rpc.mockImplementation((method: string) => {
      if (method === "mrm.readiness") {
        return Promise.resolve({
          workspace_ready: true,
          chat_ready: true,
          agents_ready: false,
          all_ready: false,
          roles: [
            { role: "chat", configured: true, ready: true, status: "ready" },
            {
              role: "planning",
              configured: true,
              ready: false,
              status: "missing_credential",
              provider: "kimi-for-coding",
            },
          ],
        });
      }
      return Promise.resolve({});
    });
    const dispose = render(() => <Settings />, document.body);
    await vi.waitFor(() => {
      const label = [...document.body.querySelectorAll("span")].find(
        (el) => el.textContent === "当前 Workspace 的 5 个 Agent 角色均可派发",
      );
      const row = label?.parentElement;
      expect(row?.textContent).toContain("需要处理");
      expect(row?.textContent).toContain("planning: 缺少可用凭证");
    });
    dispose();
  });
});

describe("Settings 实验能力与诊断导出", () => {
  it("实验配置 RPC 部分成功：重新读取权威配置，不盲目回滚", async () => {
    h.cfg
      .mockResolvedValueOnce({
        roles: {},
        send_when_running: "queue",
        experimental: {
          automatic_knowledge_distillation: false,
          browser_automation: false,
          remote_mcp: false,
        },
      })
      .mockResolvedValueOnce({
        roles: {},
        send_when_running: "queue",
        experimental: {
          automatic_knowledge_distillation: false,
          browser_automation: false,
          remote_mcp: false,
        },
      })
      .mockResolvedValueOnce({
        roles: {},
        send_when_running: "queue",
        experimental: {
          automatic_knowledge_distillation: false,
          browser_automation: false,
          remote_mcp: true,
        },
      });
    h.rpc.mockImplementation((method: string) => {
      if (method === "config.set_experimental") {
        return Promise.reject(new Error("runtime reload failed"));
      }
      return Promise.resolve({});
    });
    const dispose = render(() => <Settings />, document.body);
    btnByText("高级").click();
    await vi.waitFor(() => {
      const toggles = experimentToggles("已关闭");
      expect(toggles).toHaveLength(3);
      expect(toggles.every((button) => !button.disabled)).toBe(true);
    });

    const remoteToggle = experimentToggles("已关闭")[2];
    if (!remoteToggle) throw new Error("remote MCP toggle not found");
    remoteToggle.click();

    await vi.waitFor(() => {
      expect(h.cfg).toHaveBeenCalledTimes(3);
      expect(remoteToggle.textContent).toBe("已启用");
      expect(remoteToggle.disabled).toBe(false);
    });
    expect(flash.msgs().some((message) => message.text.includes("runtime reload failed"))).toBe(
      true,
    );
    dispose();
  });

  it("展示逐 Session Workspace 路由边界，启用实验能力并成功导出诊断", async () => {
    h.cfg.mockResolvedValue({
      roles: {},
      send_when_running: "interrupt",
      experimental: {
        automatic_knowledge_distillation: false,
        browser_automation: false,
        remote_mcp: false,
      },
    });
    h.rpc.mockImplementation((method: string) => {
      if (method === "doctor") return Promise.resolve({ entries: [], system: null });
      if (method === "diagnostics.export") {
        return Promise.resolve({ path: "/tmp/kxen-diagnostics.md" });
      }
      return Promise.resolve({});
    });
    const dispose = render(() => <Settings />, document.body);
    btnByText("高级").click();
    // hint 文案是静态的，必须等配置读回（toggle 解禁）后再点，否则点击被 disabled 吞掉
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain("按各 Session 所属 Workspace 的模型路由");
      const disabledToggles = experimentToggles("已关闭");
      expect(disabledToggles).toHaveLength(3);
      expect(disabledToggles.every((button) => !button.disabled)).toBe(true);
    });
    expect(h.rpc.mock.calls.some(([method]) => method === "current_model")).toBe(false);
    const toggles = experimentToggles("已关闭");
    expect(toggles).toHaveLength(3);
    toggles[0]!.click();
    await vi.waitFor(() =>
      expect(h.rpc).toHaveBeenCalledWith("config.set_experimental", {
        key: "automatic_knowledge_distillation",
        enabled: true,
      }),
    );
    expect(document.body.textContent).toContain("已启用");

    btnByText("导出诊断包（markdown）").click();
    await vi.waitFor(() =>
      expect(
        flash.msgs().some((message) => message.text.includes("/tmp/kxen-diagnostics.md")),
      ).toBe(true),
    );
    dispose();
  });

  it("实验能力保存失败回滚，诊断导出失败显示错误", async () => {
    h.cfg.mockResolvedValue({
      roles: {},
      experimental: {
        automatic_knowledge_distillation: true,
        browser_automation: true,
        remote_mcp: true,
      },
    });
    h.rpc.mockImplementation((method: string) => {
      if (method === "doctor") return Promise.resolve({ entries: [] });
      if (method === "current_model") return Promise.reject(new Error("no model"));
      if (method === "config.set_experimental") return Promise.reject("read only");
      if (method === "diagnostics.export") return Promise.reject(new Error("export denied"));
      return Promise.resolve({});
    });
    const dispose = render(() => <Settings />, document.body);
    btnByText("高级").click();
    await vi.waitFor(() => {
      const enabled = experimentToggles("已启用");
      expect(enabled).toHaveLength(3);
    });
    const enabled = experimentToggles("已启用");
    enabled[1]!.click();
    await vi.waitFor(() =>
      expect(flash.msgs().some((message) => message.text.includes("read only"))).toBe(true),
    );
    expect(experimentToggles("已启用")).toHaveLength(3);

    btnByText("导出诊断包（markdown）").click();
    await vi.waitFor(() =>
      expect(flash.msgs().some((message) => message.text === "导出诊断包失败：export denied")).toBe(
        true,
      ),
    );
    dispose();
  });
});
