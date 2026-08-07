// 空格 PTT 状态机实测（webkit 真实按键）：长按进语音无连打空格；失败干净回退。
// 底座是 textarea（TextComposer 同款），PTT 只依赖 getText/setText 接口。
import { describe, expect, it } from "vitest";
import type { VoiceSession } from "../../lib/voice";
import { COMPOSER_INTERRUPT_EVENT } from "../../lib/composer-bus";
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

  it("partial 只替换上屏区间：录音中手打的内容不被 partial/终稿覆盖", async () => {
    const el = mountTa("hello");
    let onPartial: (t: string) => void = () => {};
    const ctl = createVoicePtt({
      getText: () => el.value,
      setText: (v) => (el.value = v),
      afterChange: () => {},
      setRecording: () => {},
      setError: () => {},
      engine: () => "apple",
      startSession: async (_e, p) => {
        onPartial = p;
        return { engine: "apple", stop: async () => "终稿" };
      },
    });
    ctl.toggle();
    await new Promise((r) => setTimeout(r, 50));
    onPartial("世界");
    expect(el.value).toBe("hello世界");
    // 录音中手打：旧实现 setText(base+partial) 会把 "abc" 抹掉
    el.value = "hello世界abc";
    onPartial("世界和平");
    expect(el.value).toBe("hello世界和平abc");
    // 终稿同样只替换 partial 区间
    await ctl.stop();
    expect(el.value).toBe("hello终稿abc");
    // 停止后迟到的 partial 不上屏
    onPartial("追加");
    expect(el.value).toBe("hello终稿abc");
    el.remove();
  });

  it("启动中 toggle = 取消：启动落定后自停，迟到终稿不上屏", async () => {
    const el = mountTa();
    let recording = false;
    let innerStopped = 0;
    let resolveStart: (s: VoiceSession) => void = () => {};
    const ctl = createVoicePtt({
      getText: () => el.value,
      setText: (v) => (el.value = v),
      afterChange: () => {},
      setRecording: (v) => (recording = v),
      setError: () => {},
      engine: () => "apple",
      // 权限弹窗未决：start 最长挂 60s
      startSession: () =>
        new Promise<VoiceSession>((res) => {
          resolveStart = res;
        }),
    });
    ctl.toggle(); // 启动
    ctl.toggle(); // 启动中再按 = 取消（旧实现被 start 守卫吞掉，60s 内不可取消）
    resolveStart({
      engine: "apple",
      stop: async () => {
        innerStopped++;
        return "迟到终稿";
      },
    });
    await new Promise((r) => setTimeout(r, 50));
    expect(recording).toBe(false);
    expect(innerStopped).toBe(1); // 启动落定后自停，引擎不留占麦
    expect(el.value).toBe(""); // 已取消会话的迟到终稿不上屏
    el.remove();
  });

  it("start 失败后继续按住：repeat 空格不再被吞（pttTimer 已清）", async () => {
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
    key(el, "keydown");
    await new Promise((r) => setTimeout(r, 500)); // 激活计时触发 + start 失败落定
    expect(errMsg).toContain("引擎不可用");
    // 失败前 repeat 被吞（激活期判定靠 pttTimer）；失败后必须放行成普通空格
    const e = new KeyboardEvent("keydown", {
      key: " ",
      bubbles: true,
      cancelable: true,
      repeat: true,
    });
    el.dispatchEvent(e);
    expect(e.defaultPrevented).toBe(false);
    ctl.dispose();
    el.remove();
  });

  it("语音错误自动消退，权限类错误补系统设置指引", async () => {
    const el = mountTa();
    const errs: string[] = [];
    const ctl = createVoicePtt({
      getText: () => el.value,
      setText: (v) => (el.value = v),
      afterChange: () => {},
      setRecording: () => {},
      setError: (m) => errs.push(m),
      engine: () => "apple",
      errTtlMs: 60,
      startSession: async () => {
        throw new Error("麦克风权限未授予");
      },
    });
    ctl.toggle();
    await new Promise((r) => setTimeout(r, 50));
    expect(errs[errs.length - 1]).toContain("麦克风权限未授予");
    expect(errs[errs.length - 1]).toContain("系统设置");
    await new Promise((r) => setTimeout(r, 120));
    expect(errs[errs.length - 1]).toBe(""); // 到时自动消退，不常驻
    ctl.dispose();
    el.remove();
  });

  it("窗口 blur / 切后台视同 keyup：停 PTT 并清激活态", async () => {
    const el = mountTa("hi");
    let stopped = 0;
    const ctl = createVoicePtt({
      getText: () => el.value,
      setText: (v) => (el.value = v),
      afterChange: () => {},
      setRecording: () => {},
      setError: () => {},
      engine: () => "apple",
      startSession: async () => ({
        engine: "apple",
        stop: async () => {
          stopped++;
          return "世界";
        },
      }),
    });
    el.addEventListener("keydown", (e) => ctl.onSpaceDown(e));
    el.addEventListener("keyup", (e) => ctl.onSpaceUp(e));
    key(el, "keydown");
    await new Promise((r) => setTimeout(r, 500)); // PTT 激活，录音中
    window.dispatchEvent(new Event("blur"));
    await new Promise((r) => setTimeout(r, 50));
    expect(stopped).toBe(1);
    expect(el.value).toBe("hi世界");
    // 激活态已清：之后空格正常入字不被吞
    const e = new KeyboardEvent("keydown", { key: " ", bubbles: true, cancelable: true });
    el.dispatchEvent(e);
    expect(e.defaultPrevented).toBe(false);
    Object.defineProperty(document, "hidden", { value: true, configurable: true });
    try {
      key(el, "keydown");
      await new Promise((r) => setTimeout(r, 500));
      document.dispatchEvent(new Event("visibilitychange"));
      await new Promise((r) => setTimeout(r, 50));
      expect(stopped).toBe(2);
    } finally {
      delete (document as unknown as { hidden?: boolean }).hidden;
    }
    ctl.dispose();
    el.remove();
  });

  it("浮层打断事件：未决激活计时作废；录音中收到即停", async () => {
    const el = mountTa("hi");
    let started = 0;
    let stopped = 0;
    const ctl = createVoicePtt({
      getText: () => el.value,
      setText: (v) => (el.value = v),
      afterChange: () => {},
      setRecording: () => {},
      setError: () => {},
      engine: () => "apple",
      startSession: async () => {
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
    // 激活计时未决（<400ms）时被打断：计时作废，不会莫名开录
    key(el, "keydown");
    window.dispatchEvent(new CustomEvent(COMPOSER_INTERRUPT_EVENT));
    await new Promise((r) => setTimeout(r, 500));
    expect(started).toBe(0);
    // 录音中被打断：停录，终稿并入输入框
    key(el, "keydown");
    await new Promise((r) => setTimeout(r, 500));
    expect(started).toBe(1);
    window.dispatchEvent(new CustomEvent(COMPOSER_INTERRUPT_EVENT));
    await new Promise((r) => setTimeout(r, 50));
    expect(stopped).toBe(1);
    expect(el.value).toBe("hi世界");
    ctl.dispose();
    el.remove();
  });
});
