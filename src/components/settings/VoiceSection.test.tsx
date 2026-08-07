// VoiceSection 回归：ov 加载失败显错误态不裸空白；转写 key 列表按后端总览动态列（不硬编码
// openai/xai，apple 与 custom:* 不出 key 输入）；saveKey/switchEngine 失败 flashErr；
// 降级链勾选与 locale 切换走 voice.set_engine 带全量参数。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { VoiceOverview } from "../../lib/voice";

const h = vi.hoisted(() => ({
  engines: vi.fn(async () => null as unknown as VoiceOverview),
  setEngine: vi.fn(async (_e: string, _f: string[], _l?: string) => {}),
  setKey: vi.fn(async (_p: string, _k: string) => {}),
}));

vi.mock("../../lib/voice", () => ({
  voiceEngines: h.engines,
  setVoiceEngine: h.setEngine,
  setVoiceProviderKey: h.setKey,
}));

import VoiceSection from "./VoiceSection";
import { flash } from "../../lib/flash";

const OV: VoiceOverview = {
  engine: "apple",
  fallback: [],
  locale: "zh-CN",
  engines: [
    { id: "apple", label: "Apple 本地识别", status: "ready", detail: "系统 Speech" },
    { id: "openai", label: "OpenAI 转写", status: "ready", detail: "API key 已配置" },
    {
      id: "custom:relay",
      label: "relay 转写（自定义）",
      status: "unconfigured",
      detail: "未配置 API key",
    },
  ],
};

function btnByText(text: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent === text,
  );
  if (!found) throw new Error(`button not found: ${text}`);
  return found;
}

afterEach(() => {
  document.body.innerHTML = "";
  for (const m of flash.msgs()) flash.dismiss(m.id);
  vi.clearAllMocks();
});

describe("VoiceSection 加载", () => {
  it("ov 加载失败：错误态 + 重试，不留裸空白", async () => {
    h.engines.mockRejectedValueOnce(new Error("ws closed"));
    const dispose = render(() => <VoiceSection />, document.body);
    await vi.waitFor(() =>
      expect(document.body.textContent).toContain("加载语音配置失败：ws closed"),
    );
    h.engines.mockResolvedValue(OV);
    btnByText("重试").click();
    // 「识别语言」只在 ov() 到手后渲染（表头文案常驻，不能当加载完成信号）
    await vi.waitFor(() => expect(document.body.textContent).toContain("识别语言"));
    expect(document.body.textContent).not.toContain("加载语音配置失败");
    dispose();
  });
});

describe("VoiceSection 转写 key 列表", () => {
  it("按后端总览动态列：apple 与 custom:* 不出 key 输入", async () => {
    h.engines.mockResolvedValue(OV);
    const dispose = render(() => <VoiceSection />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("openai 转写 key"));
    expect(document.body.textContent).not.toContain("apple 转写 key");
    expect(document.body.textContent).not.toContain("custom:relay 转写 key");
    dispose();
  });

  it("saveKey 失败 flashErr 带原因", async () => {
    h.engines.mockResolvedValue(OV);
    h.setKey.mockRejectedValue(new Error("未知转写 provider: openai"));
    const dispose = render(() => <VoiceSection />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("openai 转写 key"));
    const input = document.body.querySelector<HTMLInputElement>("input[type=password]");
    if (!input) throw new Error("key input not found");
    input.value = "sk-test";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    const saveBtns = [...document.body.querySelectorAll<HTMLButtonElement>("button")].filter(
      (b) => b.textContent === "保存",
    );
    saveBtns[0]!.click();
    await vi.waitFor(() => {
      const err = flash.msgs().find((m) => m.kind === "err");
      expect(err?.text).toContain("保存 openai 转写 key 失败");
      expect(err?.text).toContain("未知转写 provider");
    });
    dispose();
  });
});

describe("VoiceSection 引擎配置", () => {
  it("勾选降级链：set_engine 带当前 engine/locale 与新链", async () => {
    h.engines.mockResolvedValue(OV);
    const dispose = render(() => <VoiceSection />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("OpenAI 转写"));
    const boxes = [...document.body.querySelectorAll<HTMLInputElement>("input[type=checkbox]")];
    const openaiBox = boxes.find((b) => !b.disabled);
    if (!openaiBox) throw new Error("fallback checkbox not found");
    openaiBox.click();
    await vi.waitFor(() => expect(h.setEngine).toHaveBeenCalledWith("apple", ["openai"], "zh-CN"));
    dispose();
  });

  it("locale 切换：set_engine 带新 locale", async () => {
    h.engines.mockResolvedValue(OV);
    const dispose = render(() => <VoiceSection />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("识别语言"));
    const select = [...document.body.querySelectorAll<HTMLSelectElement>("select")].find((s) =>
      [...s.options].some((o) => o.value === "en-US"),
    );
    if (!select) throw new Error("locale select not found");
    select.value = "en-US";
    select.dispatchEvent(new Event("change", { bubbles: true }));
    await vi.waitFor(() => expect(h.setEngine).toHaveBeenCalledWith("apple", [], "en-US"));
    dispose();
  });

  it("switchEngine 失败 flashErr", async () => {
    h.engines.mockResolvedValue(OV);
    h.setEngine.mockRejectedValue(new Error("config locked"));
    const dispose = render(() => <VoiceSection />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("OpenAI 转写"));
    btnByText("设为主引擎").click();
    await vi.waitFor(() => {
      const err = flash.msgs().find((m) => m.kind === "err");
      expect(err?.text).toContain("保存语音配置失败");
      expect(err?.text).toContain("config locked");
    });
    dispose();
  });
});
