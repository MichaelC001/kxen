// ModelPicker「跟随全局默认」：顶部项常驻；选中调 sessionFollowGlobalModel 清除覆盖；
// 跟随态 = 本地刚选过，或 session meta 无覆盖。
import { render } from "solid-js/web";
import "../../styles.css";
import { afterEach, describe, expect, it, vi } from "vitest";
import { userEvent } from "@vitest/browser/context";
import ModelPicker from "./ModelPicker";
import { setActiveSessionId, setSessions } from "../../lib/state";

const smMock = vi.hoisted(() => ({
  sessionSetModel: vi.fn(() => Promise.resolve()),
  sessionFollowGlobalModel: vi.fn(() => Promise.resolve()),
  applyDraftModel: vi.fn(() => Promise.resolve()),
}));
vi.mock("../../lib/session-model", () => smMock);

vi.mock("../../lib/models", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/models")>();
  return {
    ...orig,
    modelsCatalog: async () => [
      {
        provider: "xai",
        provider_name: "xAI",
        fetched_at: 0,
        source: "test",
        models: [
          {
            id: "grok-1",
            name: "Grok 1",
            family: "grok",
            reasoning: false,
            tool_call: true,
            attachment: false,
            modalities_in: ["text"],
            context: 128000,
            output: 4096,
          },
        ],
      },
    ],
  };
});

const SESSION = { id: "s1", title: "", directory: "", created_at: 0, updated_at: 0 };

const disposers: Array<() => void> = [];

afterEach(() => {
  for (const d of disposers.splice(0)) d();
  smMock.sessionSetModel.mockClear();
  smMock.sessionFollowGlobalModel.mockClear();
  setActiveSessionId("");
  setSessions([]);
  document.body.innerHTML = "";
});

function row(text: string): HTMLElement {
  const el = [...document.querySelectorAll<HTMLElement>(".model-row")].find((r) =>
    r.textContent?.includes(text),
  );
  if (!el) throw new Error(`row not found: ${text}`);
  return el;
}

async function openPicker() {
  // 弹层是 bottom-full（composer 形态）：宿主贴视口底部，否则弹层悬到视口外点不中
  const host = document.createElement("div");
  host.style.cssText = "position:fixed;bottom:8px;right:8px;";
  document.body.appendChild(host);
  const d = render(() => <ModelPicker />, host);
  disposers.push(() => {
    d();
    host.remove();
  });
  await userEvent.click(host.querySelector<HTMLElement>(".model-pill")!);
  await new Promise((r) => setTimeout(r, 50));
}

describe("ModelPicker 跟随全局默认 (webkit)", () => {
  it("顶部项常驻；session 无覆盖时为跟随态", async () => {
    setActiveSessionId("s1");
    setSessions([{ ...SESSION }]);
    await openPicker();
    expect(row("跟随全局默认").className).toContain("model-row-active");
  });

  it("session 有覆盖时非跟随态；点模型行写覆盖", async () => {
    setActiveSessionId("s1");
    setSessions([{ ...SESSION, model: { provider: "xai", model: "grok-1" } }]);
    await openPicker();
    expect(row("跟随全局默认").className).not.toContain("model-row-active");
    await userEvent.click(row("Grok 1"));
    expect(smMock.sessionSetModel).toHaveBeenCalledWith("s1", "xai", "grok-1");
  });

  it("点顶部项清除覆盖并转跟随态", async () => {
    setActiveSessionId("s1");
    setSessions([{ ...SESSION, model: { provider: "xai", model: "grok-1" } }]);
    await openPicker();
    await userEvent.click(row("跟随全局默认"));
    expect(smMock.sessionFollowGlobalModel).toHaveBeenCalledWith("s1");
    // 重开弹层：跟随态保持（本地选择优先于未刷新的 sessions 列表）
    await userEvent.click(document.querySelector<HTMLElement>(".model-pill")!);
    await new Promise((r) => setTimeout(r, 50));
    expect(row("跟随全局默认").className).toContain("model-row-active");
  });
});
