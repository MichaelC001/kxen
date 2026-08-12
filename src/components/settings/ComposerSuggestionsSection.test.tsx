import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  get: vi.fn(),
  setSuggestions: vi.fn(),
  setEmbedding: vi.fn(),
}));

vi.mock("../../lib/chat", () => ({
  configGet: h.get,
  configSetComposerSuggestions: h.setSuggestions,
  configSetEmbedding: h.setEmbedding,
}));

import ComposerSuggestionsSection from "./ComposerSuggestionsSection";

function toggleFor(label: string): HTMLButtonElement {
  const labelElement = [...document.body.querySelectorAll<HTMLDivElement>("div")].find(
    (element) => element.textContent === label,
  );
  const button = labelElement?.parentElement?.parentElement?.querySelector("button");
  if (!(button instanceof HTMLButtonElement)) throw new Error(`toggle not found: ${label}`);
  return button;
}

beforeEach(() => {
  h.get.mockReset();
  h.get.mockResolvedValue({
    composer_suggestions: { enabled: true, semantic: false, llm: false },
    embedding: { provider: null, model: null, base_url: null },
  });
  h.setSuggestions.mockReset();
  h.setSuggestions.mockResolvedValue({});
  h.setEmbedding.mockReset();
  h.setEmbedding.mockResolvedValue({});
});

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("ComposerSuggestionsSection", () => {
  it("读取配置并保持 Local 默认开启、远端能力默认关闭", async () => {
    const dispose = render(() => <ComposerSuggestionsSection />, document.body);

    await vi.waitFor(() => expect(toggleFor("上下文主动推荐").disabled).toBe(false));
    expect(toggleFor("上下文主动推荐").textContent).toBe("已启用");
    expect(toggleFor("Embedding semantic rerank").textContent).toBe("已关闭");
    expect(toggleFor("LLM prompt suggest").textContent).toBe("已关闭");
    expect(h.setSuggestions).not.toHaveBeenCalled();

    dispose();
  });

  it("未配置 embedding provider 时拒绝启用 semantic rerank", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const dispose = render(() => <ComposerSuggestionsSection />, document.body);

    await vi.waitFor(() => expect(toggleFor("Embedding semantic rerank").disabled).toBe(false));
    toggleFor("Embedding semantic rerank").click();

    await vi.waitFor(() =>
      expect(document.body.textContent).toContain(
        "启用 Embedding 前必须先选择并保存 embedding provider",
      ),
    );
    expect(confirm).not.toHaveBeenCalled();
    expect(h.setSuggestions).not.toHaveBeenCalled();

    dispose();
  });

  it("保存 embedding 配置并在确认后启用 semantic rerank", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const dispose = render(() => <ComposerSuggestionsSection />, document.body);

    const provider = document.body.querySelector("select");
    const inputs = document.body.querySelectorAll<HTMLInputElement>("input");
    await vi.waitFor(() => expect(provider?.disabled).toBe(false));
    if (!(provider instanceof HTMLSelectElement) || inputs.length !== 2) {
      throw new Error("embedding fields not found");
    }
    provider.value = "openai";
    provider.dispatchEvent(new Event("change", { bubbles: true }));
    inputs[0]!.value = "text-embedding-3-small";
    inputs[0]!.dispatchEvent(new InputEvent("input", { bubbles: true }));
    inputs[1]!.value = "https://api.openai.com/v1";
    inputs[1]!.dispatchEvent(new InputEvent("input", { bubbles: true }));

    const save = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent === "保存 Embedding",
    );
    if (!save) throw new Error("save embedding button not found");
    save.click();
    await vi.waitFor(() =>
      expect(h.setEmbedding).toHaveBeenCalledWith(
        "openai",
        "text-embedding-3-small",
        "https://api.openai.com/v1",
      ),
    );

    toggleFor("Embedding semantic rerank").click();
    await vi.waitFor(() => expect(h.setSuggestions).toHaveBeenCalledWith("semantic", true));
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(toggleFor("Embedding semantic rerank").textContent).toBe("已启用");

    dispose();
  });
});
