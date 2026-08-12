import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ rpc: vi.fn(), ok: vi.fn(), err: vi.fn(), changed: vi.fn() }));
vi.mock("../../lib/client", () => ({ client: { rpc: h.rpc } }));
vi.mock("../../lib/flash", () => ({ flashOk: h.ok, flashErr: h.err }));

import BotRecovery from "./BotRecovery";
import BotRoutines from "./BotRoutines";
import BotRuns from "./BotRuns";

const definition = {
  display_name: "Runtime Bot",
  description: "Runtime tests",
  objective: "Execute",
  success_criteria: ["PASS"],
  instructions: "Return evidence",
  input_contract: { description: "input", content_type: "text/plain", required_fields: [] },
  output_contract: { description: "output", content_type: "text/plain", required_fields: [] },
  mrm_role: "execution",
  capabilities: ["read"],
  resources: { workspaces: [], connectors: [] },
  approval: "ask",
  budget: {},
  context: {},
  memory: { enabled: true, max_items: 10, allow_sensitive: false },
  communication: { allow_direct: true, allow_groups: true, allowed_peers: [] },
  failure: { max_pure_retries: 1, auto_pause_after_failures: 3 },
};
const bot = {
  bot_id: "bot_runtime",
  lifecycle: "active",
  event_version: 4,
  draft_version_counter: 1,
  draft: { version: 1, content_hash: "hash", definition },
  current_revision_id: "revision_runtime",
  revisions: {
    revision_runtime: {
      revision_id: "revision_runtime",
      revision_number: 1,
      content_hash: "hash",
      definition,
    },
  },
  created_at_ms: 1,
  updated_at_ms: 2,
};
const run = {
  spec: {
    run_id: "run_runtime",
    bot_id: "bot_runtime",
    revision_id: "revision_runtime",
    trigger: { kind: "manual" },
  },
  status: "awaiting_input",
  event_version: 5,
  result: [{ kind: "text", text: "Result text" }, { kind: "data" }],
  approval: { approval_id: "approval_1", operation_id: "operation_1", summary: "Publish report" },
  input_request: { request_id: "input_1", prompt: "Need scope" },
  artifacts: [
    {
      artifact_id: "artifact_1",
      display_name: "report.md",
      media_type: "text/markdown",
      content_hash: "artifact_hash",
      size_bytes: 6,
    },
  ],
  error_code: "waiting",
  error_message: "owner action required",
  usage: { input_tokens: 10, output_tokens: 4, tool_calls: 2, turns: 1, wall_clock_ms: 20 },
  updated_at_ms: 20,
};
const routine = {
  routine_id: "routine_daily",
  lifecycle: "active",
  event_version: 3,
  definition: {
    bot_id: "bot_runtime",
    name: "Daily report",
    schedule: {
      expression: { kind: "cron", expression: "0 9 * * *" },
      timezone: "Asia/Dubai",
      misfire: "run_once",
      max_lateness_ms: 300000,
    },
    context_mode: "isolated",
    input: [{ kind: "text", text: "Daily input" }],
    revision_policy: { kind: "follow_current" },
    failure_threshold: 3,
  },
  next_scheduled_at_ms: 1786545000000,
  consecutive_failures: 1,
  occurrences: {
    occurrence_1: {
      occurrence_id: "occurrence_1",
      status: "completed",
      manual: true,
      run_id: "run_runtime",
      observed_at_ms: 1,
    },
  },
};
const conversation = {
  conversation_id: "conversation_1",
  kind: "bot_group",
  lifecycle: "active",
  event_version: 1,
  members: { bot_runtime: { bot_id: "bot_runtime", active: true } },
  messages: [],
  message_sequences: {},
  tasks: {},
};

function button(text: string, index = 0) {
  return [...document.body.querySelectorAll<HTMLButtonElement>("button")].filter((item) =>
    item.textContent?.includes(text),
  )[index]!;
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
  await vi.waitFor(() => expect(h.rpc.mock.calls.map((call) => call[0])).toContain(method));
}
async function press(text: string, index = 0) {
  await vi.waitFor(() => expect(button(text, index).disabled).toBe(false));
  button(text, index).click();
}

beforeEach(() => {
  h.rpc.mockReset();
  h.rpc.mockImplementation((method: string) => {
    if (method === "bot.list")
      return Promise.resolve([
        {
          bot_id: "bot_runtime",
          display_name: "Runtime Bot",
          lifecycle: "active",
          current_revision_id: "revision_runtime",
          updated_at_ms: 1,
        },
      ]);
    if (method === "bot.get") return Promise.resolve(bot);
    if (method === "bot.conversation.list") return Promise.resolve([conversation]);
    if (method === "bot.routine.list")
      return Promise.resolve([
        routine,
        { ...routine, routine_id: "routine_paused", lifecycle: "paused" },
      ]);
    if (method === "bot.run.list") return Promise.resolve([run]);
    if (method === "bot.artifact.get") return Promise.resolve({ content_base64: btoa("report") });
    if (method === "bot.recovery.inspect")
      return Promise.resolve({
        registry: [
          {
            recovery_id: "recovery_bot",
            aggregate: { kind: "bot", id: "bot_runtime" },
            reason: "repair bot",
            evidence: ["event 3"],
            opened_at_ms: 1,
          },
          {
            recovery_id: "recovery_run",
            aggregate: { kind: "bot_run", id: "run_runtime" },
            reason: "unknown effect",
            evidence: ["operation 1"],
            opened_at_ms: 2,
          },
          {
            recovery_id: "recovery_routine",
            aggregate: { kind: "routine", id: "routine_daily" },
            reason: "blocked",
            evidence: [],
            opened_at_ms: 3,
          },
        ],
        bots: [{ ...bot, lifecycle: "blocked", blocked_reason: "repair bot" }],
        runs: [{ ...run, status: "blocked" }],
        conversations: [{ ...conversation, lifecycle: "blocked" }],
        routines: [{ ...routine, lifecycle: "blocked", blocked_reason: "blocked routine" }],
      });
    return Promise.resolve(run);
  });
});
afterEach(() => {
  document.body.innerHTML = "";
});

describe("Bot runtime surfaces", () => {
  it("creates and controls routines using the published input contract", async () => {
    const dispose = render(() => <BotRoutines epoch={0} onChanged={h.changed} />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("Daily report"));
    await press("编辑");
    await vi.waitFor(() =>
      expect(
        (document.body.querySelector('input[placeholder="Routine 名称"]') as HTMLInputElement)
          .value,
      ).toBe("Daily report"),
    );
    await press("保存");
    await called("bot.routine.update");
    await press("Run now");
    await called("bot.routine.run_now");
    await press("Pause");
    await called("bot.routine.pause");
    await press("Resume");
    await called("bot.routine.resume");
    await press("Trash");
    await called("bot.routine.trash");
    const selects = [...document.body.querySelectorAll<HTMLSelectElement>("select")];
    choose(selects[0]!, "bot_runtime");
    fill(document.body.querySelector('input[placeholder="Routine 名称"]')!, "New routine");
    fill(document.body.querySelector('textarea[placeholder="每次运行的输入"]')!, "New input");
    await vi.waitFor(() => expect(button("创建").disabled).toBe(false));
    await press("创建");
    await called("bot.routine.create");
    dispose();
  });

  it("handles approval, input, cancellation and immutable artifacts", async () => {
    const dispose = render(() => <BotRuns epoch={0} onChanged={h.changed} />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("Publish report"));
    await press("Allow");
    await called("bot.run.approval");
    fill(
      document.body.querySelector('input[placeholder="补充本次 Run 所需信息"]')!,
      "workspace scope",
    );
    await press("提交");
    await called("bot.run.input");
    await press("Cancel Run");
    await called("bot.run.cancel");
    await press("验证并预览");
    await vi.waitFor(() => expect(document.body.textContent).toContain("report"));
    await press("Trash");
    await called("bot.artifact.trash");
    await press("Restore");
    await called("bot.artifact.restore");
    dispose();
  });

  it("requires explicit repair or clear decisions for UNKNOWN state", async () => {
    const dispose = render(() => <BotRecovery epoch={0} onChanged={h.changed} />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("unknown effect"));
    expect(document.body.textContent).toContain("Blocked Conversations");
    await press("证据已修复");
    await called("bot.recovery.repair");
    await press("确认放弃，不重试 UNKNOWN effect");
    await called("bot.recovery.clear");
    expect(h.err).not.toHaveBeenCalled();
    dispose();
  });
});
