import { createRoot } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ComposerSuggestion } from "../../lib/chat";
import { createAutoSuggest } from "./auto-suggest";

const localItem: ComposerSuggestion = {
  id: "file:src/auth.rs",
  kind: "file",
  path: "src/auth.rs",
  label: "auth.rs",
  reason: "匹配完整输入",
  source: "local",
  score: 1,
};

function harness(remoteEnabled = false) {
  let draft = "修复 token 刷新";
  let sessionId = "ses_test";
  let caretAtEnd = true;
  let blocked = false;
  const added: string[] = [];
  const inserted: string[] = [];
  const local = vi.fn(async () => ({ suggestions: [localItem], trusted: true }));
  const remote = vi.fn(
    async (): Promise<{ suggestions: ComposerSuggestion[]; warnings: string[] }> => ({
      suggestions: [],
      warnings: [],
    }),
  );
  const cancel = vi.fn(async () => {});
  let disposeRoot = () => {};
  const controller = createRoot((dispose) => {
    disposeRoot = dispose;
    return createAutoSuggest(
      {
        text: () => draft,
        sessionId: () => sessionId,
        selectedPaths: () => ["src/current.rs"],
        caretAtEnd: () => caretAtEnd,
        blocked: () => blocked,
        imeLocked: () => false,
        addFile: (path) => added.push(path),
        insertText: (text) => inserted.push(text),
        focus: () => {},
      },
      {
        config: async () => ({
          roles: {},
          composer_suggestions: { enabled: true, semantic: remoteEnabled, llm: false },
        }),
        local,
        remote,
        cancel,
      },
    );
  });
  return {
    controller,
    local,
    remote,
    cancel,
    added,
    inserted,
    setDraft: (value: string) => (draft = value),
    setSessionId: (value: string) => (sessionId = value),
    setCaretAtEnd: (value: boolean) => (caretAtEnd = value),
    setBlocked: (value: boolean) => (blocked = value),
    dispose: () => {
      controller.dispose();
      disposeRoot();
    },
  };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("Composer auto suggest", () => {
  it("普通文本无需触发符即可按完整 draft、Session 和附件请求本地候选", async () => {
    vi.useFakeTimers();
    const h = harness();
    await vi.advanceTimersByTimeAsync(350);
    expect(h.local).toHaveBeenCalledWith("修复 token 刷新", "ses_test", ["src/current.rs"], 6);
    expect(h.controller.state()?.items).toEqual([localItem]);
    h.dispose();
  });

  it("光标不在末尾或处于阻塞状态时不会请求并会关闭候选", async () => {
    vi.useFakeTimers();
    const h = harness();
    await vi.advanceTimersByTimeAsync(350);
    h.setCaretAtEnd(false);
    h.controller.run();
    expect(h.controller.state()).toBeNull();
    h.setCaretAtEnd(true);
    h.setBlocked(true);
    h.controller.run();
    await vi.advanceTimersByTimeAsync(400);
    expect(h.local).toHaveBeenCalledTimes(1);
    h.dispose();
  });

  it("新 draft 会取消在飞远端请求并丢弃旧结果", async () => {
    vi.useFakeTimers();
    const h = harness(true);
    let resolveRemote!: (value: { suggestions: ComposerSuggestion[]; warnings: string[] }) => void;
    h.remote.mockImplementationOnce(() => new Promise((resolve) => (resolveRemote = resolve)));
    await vi.advanceTimersByTimeAsync(350);
    vi.advanceTimersByTime(550);
    await Promise.resolve();
    expect(h.remote).toHaveBeenCalledTimes(1);
    h.setDraft("修复另一个问题");
    h.controller.run();
    expect(h.cancel).toHaveBeenCalledTimes(1);
    resolveRemote({
      suggestions: [{ ...localItem, id: "file:stale", path: "stale" }],
      warnings: [],
    });
    await Promise.resolve();
    expect(h.controller.state()).toBeNull();
    h.dispose();
  });

  it("Tab 接受候选，Enter 保持发送语义", async () => {
    vi.useFakeTimers();
    const h = harness();
    await vi.advanceTimersByTimeAsync(350);
    const enter = new KeyboardEvent("keydown", { key: "Enter", cancelable: true });
    expect(h.controller.handleKey(enter)).toBe(false);
    const tab = new KeyboardEvent("keydown", { key: "Tab", cancelable: true });
    expect(h.controller.handleKey(tab)).toBe(true);
    expect(h.added).toEqual(["src/auth.rs"]);
    expect(h.controller.state()).toBeNull();
    h.dispose();
  });
});
