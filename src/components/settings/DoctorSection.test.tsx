// DoctorSection：诊断区结构化呈现 doctor RPC 数据（凭证/MCP/LSP/MRM/event bus）。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DoctorReport } from "../../lib/chat";

const h = vi.hoisted(() => ({
  doctor: vi.fn<() => Promise<DoctorReport>>(),
}));

vi.mock("../../lib/chat", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/chat")>();
  return { ...orig, doctor: h.doctor };
});

import DoctorSection from "./DoctorSection";

function report(): DoctorReport {
  return {
    runtime: "0.1.0",
    data_dir: "/tmp/data",
    config_dir: "/tmp/config",
    entries: [
      { provider: "anthropic", display: "Claude", status: "ok", detail: "" },
      { provider: "xai", display: "Grok", status: "expired", detail: "" },
      { provider: "kimi", display: "Kimi", status: "missing", detail: "" },
    ],
    system: {
      mcp: [
        {
          name: "fs",
          status: "running",
          transport: "stdio",
          url: null,
          tools: 5,
          resources: 2,
          prompts: [],
          last_auth_error: null,
        },
      ],
      lsp_root: "/tmp/proj",
      lsp: [{ language: "rust", status: "running" }],
      mrm_describe: "global limit: 8",
      mrm_dispatches: 3,
      bus_capacity: 256,
      bus_receivers: 0,
    },
  };
}

afterEach(() => {
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

describe("DoctorSection", () => {
  it("凭证三态 + 子系统健康全呈现，bus 0 订阅标异常", async () => {
    h.doctor.mockResolvedValue(report());
    const dispose = render(() => <DoctorSection />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("rust"));
    const text = document.body.textContent ?? "";
    for (const expected of [
      "Claude",
      "已过期",
      "未配置",
      "fs",
      "rust",
      "global limit: 8",
      "累计派发 3",
      "异常：无订阅者",
    ]) {
      expect(text).toContain(expected);
    }
    dispose();
  });

  it("RPC 失败：显错误不白屏", async () => {
    h.doctor.mockRejectedValue(new Error("backend down"));
    const dispose = render(() => <DoctorSection />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("诊断数据加载失败"));
    dispose();
  });
});
