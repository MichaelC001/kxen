// 空格 PTT 状态机实测（webkit 真实按键）：长按进语音无连打空格；失败干净回退。
// 底座是 textarea（TextComposer 同款），PTT 只依赖 getText/setText 接口。
import { describe, expect, it } from "vitest";
import { createVoicePtt } from "./voice-ptt";

function key(el: HTMLElement, type: "keydown" | "keyup", repeat = false) {
  el.dispatchEvent(new KeyboardEvent(type, { key: " ", bubbles: true, cancelable: true, repeat }));
}

function mountTa(initial = "") {
  const el = document.createElement("textarea");
  el.value = initial;
  document.body.appendChild(el);
  return el;
}

describe("voice PTT (webkit)", () => {
  it("长按 500ms 进语音，输入区无连打空格", async () => {
    const el = mountTa("hello");
    let recording = false;
    let started = 0;
    let stopped = 0;
    const ctl = createVoicePtt({
      getText: () => el.value,
      setText: (v) => (el.value = v),
      afterChange: () => {},
      setRecording: (v) => (recording = v),
      setError: () => {},
      engine: () => "apple",
      startSession: async (_e, _p, _err) => {
        started++;
        return {
          engine: "apple",
          stop: async () => {
            stopped++;
            return "世界";
          },
        };
      },
    });
    el.addEventListener("keydown", (e) => ctl.onSpaceDown(e));
    el.addEventListener("keyup", (e) => ctl.onSpaceUp(e));
    el.focus();
    key(el, "keydown");
    // 激活期内连打（自动重复）：应被 preventDefault 全部拦截
    key(el, "keydown", true);
    key(el, "keydown", true);
    key(el, "keydown", true);
    await new Promise((r) => setTimeout(r, 500));
    key(el, "keyup");
    await new Promise((r) => setTimeout(r, 50));
    expect(started).toBe(1);
    expect(stopped).toBe(1);
    expect(el.value).toBe("hello世界");
    expect(recording).toBe(false);
    el.remove();
  });

  it("start 失败：无残留空格且状态复位", async () => {
    const el = mountTa("ab");
    let errMsg = "";
    const ctl = createVoicePtt({
      getText: () => el.value,
      setText: (v) => (el.value = v),
      afterChange: () => {},
      setRecording: () => {},
      setError: (m) => (errMsg = m),
      engine: () => "apple",
      startSession: async () => {
        throw new Error("引擎不可用");
      },
    });
    el.addEventListener("keydown", (e) => ctl.onSpaceDown(e));
    el.addEventListener("keyup", (e) => ctl.onSpaceUp(e));
    el.focus();
    key(el, "keydown");
    key(el, "keydown", true);
    await new Promise((r) => setTimeout(r, 500));
    key(el, "keyup");
    await new Promise((r) => setTimeout(r, 50));
    expect(errMsg).toContain("引擎不可用");
    // 激活期误输入被撤销（base 文本原样，无连续空格）
    expect(el.value.includes("  ")).toBe(false);
    expect(el.value.startsWith("ab")).toBe(true);
    el.remove();
  });

  it("短按空格正常入字（不触发语音）", async () => {
    const el = mountTa();
    let started = 0;
    const ctl = createVoicePtt({
      getText: () => el.value,
      setText: (v) => (el.value = v),
      afterChange: () => {},
      setRecording: () => {},
      setError: () => {},
      engine: () => "apple",
      startSession: async () => {
        started++;
        return { engine: "apple", stop: async () => null };
      },
    });
    el.addEventListener("keydown", (e) => ctl.onSpaceDown(e));
    el.addEventListener("keyup", (e) => ctl.onSpaceUp(e));
    el.focus();
    key(el, "keydown");
    key(el, "keyup");
    await new Promise((r) => setTimeout(r, 500));
    expect(started).toBe(0);
    el.remove();
  });

  it("startSession 收到当前 session id（多会话 PTT 键控）", async () => {
    const el = mountTa();
    let gotSid = "";
    const ctl = createVoicePtt({
      getText: () => el.value,
      setText: (v) => (el.value = v),
      afterChange: () => {},
      setRecording: () => {},
      setError: () => {},
      engine: () => "apple",
      sessionId: () => "sess-42",
      startSession: async (_e, _p, _err, sid) => {
        gotSid = sid;
        return { engine: "apple", stop: async () => null };
      },
    });
    el.addEventListener("keydown", (e) => ctl.onSpaceDown(e));
    el.addEventListener("keyup", (e) => ctl.onSpaceUp(e));
    el.focus();
    key(el, "keydown");
    await new Promise((r) => setTimeout(r, 500));
    key(el, "keyup");
    await new Promise((r) => setTimeout(r, 50));
    expect(gotSid).toBe("sess-42");
    el.remove();
  });
});
