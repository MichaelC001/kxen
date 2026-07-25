// 审批卡状态机：正常应答置决定态；后端 resolved 事件与迟到应答置失效态（不改写已决定的卡）。
import { createSignal } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";

const chatMock = vi.hoisted(() => ({ approvalRespond: vi.fn() }));
vi.mock("./chat", () => ({ approvalRespond: chatMock.approvalRespond }));

import { applyApprovalEvent, applyApprovalResolved, respondApproval } from "./approvals";
import type { ApprovalItem, Item } from "./items";

function setup() {
  const [items, setItems] = createSignal<Item[]>([]);
  applyApprovalEvent(setItems, {
    kind: "approval",
    name: "approval",
    approvalId: "a1",
    command: "rm -rf x",
    reason: "危险",
  });
  return { items, setItems, card: () => items().at(-1) as ApprovalItem };
}

beforeEach(() => chatMock.approvalRespond.mockReset());

describe("applyApprovalResolved（后端了结事件）", () => {
  it("timeout 置已超时，其他 outcome 置已取消", () => {
    const a = setup();
    applyApprovalResolved(a.setItems, "a1", "timeout");
    expect(a.card().resolved).toBe("timeout");
    const b = setup();
    applyApprovalResolved(b.setItems, "a1", "cancelled");
    expect(b.card().resolved).toBe("cancelled");
  });

  it("用户已决定的卡不被迟到事件改写；未知 id 不影响其他卡", () => {
    const a = setup();
    applyApprovalResolved(a.setItems, "nobody", "timeout");
    expect(a.card().resolved).toBeUndefined();
    applyApprovalResolved(a.setItems, "a1", "cancelled");
    expect(a.card().resolved).toBe("cancelled");
    applyApprovalResolved(a.setItems, "a1", "timeout");
    expect(a.card().resolved).toBe("cancelled");
  });
});

describe("respondApproval", () => {
  it("服务端确认应答：按用户选择置 allowed/denied", async () => {
    chatMock.approvalRespond.mockResolvedValue({ resolved: true });
    const a = setup();
    await respondApproval(a.setItems, "a1", true);
    expect(a.card().resolved).toBe("allowed");
    const b = setup();
    await respondApproval(b.setItems, "a1", false);
    expect(b.card().resolved).toBe("denied");
  });

  it("迟到应答（resolved:false）：置失效，不冒充用户决定", async () => {
    chatMock.approvalRespond.mockResolvedValue({ resolved: false });
    const a = setup();
    await respondApproval(a.setItems, "a1", true);
    expect(a.card().resolved).toBe("expired");
  });
});
