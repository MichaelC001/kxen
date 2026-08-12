import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ rpc: vi.fn(), ok: vi.fn(), err: vi.fn(), changed: vi.fn() }));
vi.mock("../../lib/client", () => ({ client: { rpc: h.rpc } }));
vi.mock("../../lib/flash", () => ({ flashOk: h.ok, flashErr: h.err }));

import BotCollaboration from "./BotCollaboration";

const bots = ["alpha", "beta", "gamma", "delta"].map((id, index) => ({
  bot_id: `bot_${id}`,
  display_name: id.toUpperCase(),
  lifecycle: "active",
  current_revision_id: `revision_${id}`,
  updated_at_ms: index,
}));
const conversation = {
  conversation_id: "conversation_group",
  kind: "bot_group",
  lifecycle: "active",
  event_version: 9,
  moderator_bot_id: "bot_alpha",
  members: {
    bot_alpha: { bot_id: "bot_alpha", active: true },
    bot_beta: { bot_id: "bot_beta", active: true },
    bot_gamma: { bot_id: "bot_gamma", active: true },
  },
  messages: [
    {
      message_id: "message_1",
      kind: "request",
      actor: { kind: "owner" },
      parts: [{ kind: "text", text: "Prepare release" }],
      created_at_ms: 1,
    },
    {
      message_id: "message_2",
      kind: "response",
      actor: { kind: "bot", id: "bot_beta" },
      parts: [{ kind: "data", schema_id: "report", fields: { status: "PASS" } }],
      created_at_ms: 2,
    },
    {
      message_id: "message_3",
      kind: "notice",
      actor: { kind: "system", actor: "dispatcher" },
      parts: [
        {
          kind: "artifact_ref",
          artifact: {
            artifact_id: "artifact_1",
            display_name: "report.md",
            media_type: "text/markdown",
            content_hash: "hash",
            size_bytes: 12,
          },
        },
      ],
      created_at_ms: 3,
    },
  ],
  message_sequences: { message_1: 1, message_2: 2, message_3: 3 },
  tasks: {
    task_1: {
      task_id: "task_1",
      conversation_id: "conversation_group",
      title: "Review",
      owner_bot_id: "bot_beta",
      status: "running",
    },
    task_2: {
      task_id: "task_2",
      conversation_id: "conversation_group",
      title: "Done",
      owner_bot_id: "bot_gamma",
      status: "completed",
    },
  },
};

function buttons(text: string) {
  return [...document.body.querySelectorAll<HTMLButtonElement>("button")].filter((item) =>
    item.textContent?.includes(text),
  );
}
function fill(element: HTMLInputElement | HTMLTextAreaElement, value: string) {
  element.value = value;
  element.dispatchEvent(new InputEvent("input", { bubbles: true, data: value }));
}
function choose(element: HTMLSelectElement, value: string) {
  element.value = value;
  element.dispatchEvent(new Event("change", { bubbles: true }));
}
async function called(method: string) {
  await vi.waitFor(() => {
    const methods = h.rpc.mock.calls.map((call) => call[0]);
    expect(methods, `RPC calls: ${methods.join(", ")}`).toContain(method);
  });
}
async function press(text: string, index = 0) {
  await vi.waitFor(() => expect(buttons(text)[index]?.disabled).toBe(false));
  buttons(text)[index]!.click();
}
async function invoke(text: string, method: string, index = 0) {
  const completed = h.ok.mock.calls.length;
  await press(text, index);
  await called(method);
  await vi.waitFor(() => expect(h.ok.mock.calls.length).toBe(completed + 1));
}

beforeEach(() => {
  h.rpc.mockReset();
  h.ok.mockReset();
  h.err.mockReset();
  h.changed.mockReset();
  h.rpc.mockImplementation((method: string) => {
    if (method === "bot.list") return Promise.resolve(bots);
    if (method === "bot.conversation.list") return Promise.resolve([conversation]);
    if (method === "bot.conversation.get") return Promise.resolve(conversation);
    if (method === "bot.direct.open")
      return Promise.resolve({
        ...conversation,
        conversation_id: "conversation_direct",
        kind: "bot_direct",
      });
    return Promise.resolve(conversation);
  });
});
afterEach(() => {
  document.body.innerHTML = "";
});

describe("Bot collaboration", () => {
  it("creates groups and direct conversations with explicit members", async () => {
    const dispose = render(
      () => <BotCollaboration epoch={0} onChanged={h.changed} />,
      document.body,
    );
    await vi.waitFor(() => expect(document.body.textContent).toContain("Prepare release"));
    const memberInputs = [
      ...document.body.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'),
    ];
    memberInputs[0]!.click();
    memberInputs[1]!.click();
    const selects = [...document.body.querySelectorAll<HTMLSelectElement>("select")];
    choose(selects[0]!, "bot_alpha");
    await invoke("创建 Group", "bot.group.create");
    const currentSelects = [...document.body.querySelectorAll<HTMLSelectElement>("select")];
    choose(currentSelects[1]!, "bot_alpha");
    choose(currentSelects[2]!, "bot_beta");
    await invoke("打开 Direct", "bot.direct.open");
    dispose();
  });

  it("posts group work and applies lifecycle, membership and task actions", async () => {
    const dispose = render(
      () => <BotCollaboration epoch={0} onChanged={h.changed} />,
      document.body,
    );
    await vi.waitFor(() => expect(document.body.textContent).toContain("Timeline 与 Tasks"));
    expect(document.body.textContent).toContain("PASS");
    expect(document.body.textContent).toContain("report.md");
    const textarea = document.body.querySelector<HTMLTextAreaElement>(
      'textarea[placeholder="说明协作目标和期望产物"]',
    )!;
    fill(textarea, "Ship the release");
    const groupChecks = [
      ...document.body.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'),
    ];
    groupChecks.at(-1)!.click();
    await invoke("投递指令", "bot.conversation.post");
    const addSelect = [...document.body.querySelectorAll<HTMLSelectElement>("select")].at(-1)!;
    choose(addSelect, "bot_delta");
    await invoke("添加", "bot.group.add_member", buttons("添加").length - 1);
    await invoke("设为 Moderator", "bot.group.set_moderator", 1);
    await invoke("移除", "bot.group.remove_member", 1);
    await invoke("Pause", "bot.conversation.pause");
    await invoke("Archive", "bot.conversation.archive");
    await invoke("Stop Group", "bot.group.stop");
    await invoke("Cancel", "bot.task.cancel");
    expect(h.err).not.toHaveBeenCalled();
    dispose();
  });
});
