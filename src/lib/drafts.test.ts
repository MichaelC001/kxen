// drafts store：新会话稳定键、首发迁移、发送后清理、会话间隔离。
import { beforeEach, describe, expect, it } from "vitest";
import { DRAFT_NEW, clearDraft, draftKey, getDraft, migrateNewDraft, setDraft } from "./drafts";

beforeEach(() => {
  clearDraft("");
  clearDraft("s1");
  clearDraft("s2");
});

describe("drafts store", () => {
  it("新会话统一落在稳定键 DRAFT_NEW", () => {
    expect(draftKey("")).toBe(DRAFT_NEW);
    setDraft("", "hello");
    expect(getDraft("")).toBe("hello");
  });

  it("首发落库后迁移到真实 id 并清空旧键", () => {
    setDraft("", "wip");
    migrateNewDraft("s1");
    expect(getDraft("")).toBe("");
    expect(getDraft("s1")).toBe("wip");
  });

  it("迁移不覆盖真实 id 已有草稿", () => {
    setDraft("", "pending");
    setDraft("s1", "own");
    migrateNewDraft("s1");
    expect(getDraft("s1")).toBe("own");
    expect(getDraft("")).toBe("");
  });

  it("新会话首发后旧键清空，下次新会话不恢复已发送内容", () => {
    setDraft("", "first message");
    // 发送成功：发送时真实 id 还没回来，清的是 DRAFT_NEW
    clearDraft("");
    migrateNewDraft("s1");
    expect(getDraft("")).toBe("");
    expect(getDraft("s1")).toBe("");
  });

  it("首发在途继续打字的内容随迁移保留到真实会话", () => {
    setDraft("", "sent");
    clearDraft("");
    setDraft("", "still typing");
    migrateNewDraft("s1");
    expect(getDraft("")).toBe("");
    expect(getDraft("s1")).toBe("still typing");
  });

  it("切换会话不串草稿", () => {
    setDraft("", "new session wip");
    setDraft("s1", "s1 wip");
    expect(getDraft("s2")).toBe("");
    expect(getDraft("s1")).toBe("s1 wip");
    expect(getDraft("")).toBe("new session wip");
  });
});
