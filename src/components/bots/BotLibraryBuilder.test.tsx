import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ rpc: vi.fn(), ok: vi.fn(), err: vi.fn(), changed: vi.fn() }));
vi.mock("../../lib/client", () => ({ client: { rpc: h.rpc } }));
vi.mock("../../lib/flash", () => ({ flashOk: h.ok, flashErr: h.err }));

import BotBuilder from "./BotBuilder";
import BotLibrary from "./BotLibrary";

const definition = {
  display_name: "Report Bot",
  description: "Build reports",
  objective: "Create a report",
  success_criteria: ["accurate", "complete"],
  instructions: "Use evidence",
  input_contract: { description: "request", content_type: "text/plain", required_fields: [] },
  output_contract: { description: "report", content_type: "text/plain", required_fields: [] },
  mrm_role: "execution",
  capabilities: ["read", "bot_artifact"],
  resources: { workspaces: [], connectors: [] },
  approval: "ask",
  budget: { max_turns: 4 },
  context: { max_parts: 20 },
  memory: { enabled: true, max_items: 20, allow_sensitive: false },
  communication: { allow_direct: true, allow_groups: true, allowed_peers: ["bot_peer"] },
  failure: { max_pure_retries: 1, auto_pause_after_failures: 3 },
};
const state = {
  bot_id: "bot_report",
  lifecycle: "active",
  event_version: 7,
  draft_version_counter: 1,
  draft: { version: 1, content_hash: "draft_hash", definition },
  current_revision_id: "revision_report_1",
  revisions: {
    revision_report_1: {
      revision_id: "revision_report_1",
      revision_number: 1,
      content_hash: "draft_hash",
      definition,
    },
  },
  created_at_ms: 1,
  updated_at_ms: 2,
};
const builder = {
  builder_session_id: "builder_report",
  bot_id: "bot_report",
  lifecycle: "active",
  event_version: 8,
  user_goal: "Create reports",
  draft: { version: 1, source_message_id: "message_1", content_hash: "draft_hash", definition },
  grants: [
    {
      grant_id: "grant_1",
      draft_hash: "draft_hash",
      permission_hash: "permission_1",
      reason: "reviewed",
    },
  ],
  reports: [
    {
      report_id: "report_1",
      draft_hash: "draft_hash",
      publish_eligible: true,
      findings: [{ code: "contract", status: "PASS", message: "valid", evidence: "test" }],
    },
  ],
  tests: [{ run_id: "run_test", draft_hash: "draft_hash", passed: true, summary: "PASS" }],
};

function button(text: string) {
  return [...document.body.querySelectorAll<HTMLButtonElement>("button")].find((item) =>
    item.textContent?.includes(text),
  )!;
}
function input(placeholder: string) {
  return [
    ...document.body.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>("input,textarea"),
  ].find((item) => item.placeholder === placeholder)!;
}
function fill(element: HTMLInputElement | HTMLTextAreaElement, value: string) {
  element.value = value;
  element.dispatchEvent(
    new InputEvent("input", { bubbles: true, inputType: "insertText", data: value }),
  );
}
async function called(method: string) {
  await vi.waitFor(() => expect(h.rpc.mock.calls.map((call) => call[0])).toContain(method));
}
async function press(text: string) {
  await vi.waitFor(() => expect(button(text).disabled).toBe(false));
  button(text).click();
}

beforeEach(() => {
  h.rpc.mockReset();
  h.ok.mockReset();
  h.err.mockReset();
  h.changed.mockReset();
  h.rpc.mockImplementation((method: string) => {
    if (method === "bot.list")
      return Promise.resolve([
        {
          bot_id: "bot_report",
          display_name: "Report Bot",
          lifecycle: "active",
          current_revision_id: "revision_report_1",
          updated_at_ms: 2,
        },
      ]);
    if (method === "bot.get") return Promise.resolve(state);
    if (method === "bot.memory.list")
      return Promise.resolve({
        event_version: 3,
        items: {
          memory_1: {
            item_id: "memory_1",
            kind: "fact",
            content: "Keep evidence",
            version: 1,
            updated_at_ms: 1,
          },
        },
      });
    if (method === "bot.builder.get" || method.startsWith("bot.builder."))
      return Promise.resolve(builder);
    if (method === "bot.validate") return Promise.resolve(builder);
    return Promise.resolve(state);
  });
});
afterEach(() => {
  document.body.innerHTML = "";
});

describe("Bot Library and Builder", () => {
  it("runs lifecycle and memory actions against the published definition", async () => {
    const dispose = render(() => <BotLibrary epoch={0} onChanged={h.changed} />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("Report Bot"));
    expect(document.body.textContent).toContain("revision");
    fill(input("描述本次要完成的工作"), "weekly report");
    await press("运行 Bot");
    await called("bot.run.start");
    await press("Duplicate");
    await called("bot.duplicate");
    await press("Pause");
    await called("bot.pause");
    await press("Archive");
    await called("bot.archive");
    await press("移到废纸篓");
    await called("bot.trash");
    await press("编辑");
    fill(input("明确、非敏感的记忆内容"), "Updated evidence rule");
    await press("保存");
    await called("bot.memory.revise");
    await press("删除");
    await called("bot.memory.remove");
    dispose();
  });

  it("creates, revises, grants, tests, validates and publishes a draft", async () => {
    const dispose = render(() => <BotBuilder epoch={0} onChanged={h.changed} />, document.body);
    fill(input("Bot 名称"), "Report Bot");
    fill(input("要长期重复完成什么工作，输入、输出和成功标准是什么"), "Create reports");
    await press("生成 Bot 草稿");
    await called("bot.builder.start");
    await called("bot.builder.message");
    await vi.waitFor(() => expect(document.body.textContent).toContain("发布门禁"));
    fill(input("要求 Builder Agent 调整草稿"), "Use JSON output");
    await press("更新");
    await called("bot.builder.message");
    await press("授权当前权限");
    await called("bot.builder.grant");
    await press("运行受控测试");
    await called("bot.builder.test");
    await press("执行验证");
    await called("bot.validate");
    await press("发布 Bot");
    await called("bot.publish");
    await press("取消 Build");
    await called("bot.builder.cancel");
    expect(h.err).not.toHaveBeenCalled();
    dispose();
  });
});
