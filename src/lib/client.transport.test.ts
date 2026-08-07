import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: h.invoke,
}));

// 原生 WebSocket 假实现：行为队列控制每个新实例 open 还是 error；
// sendHook 对所有实例生效（断连重连会换实例，失败注入必须跨实例）。
class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  static behaviors: Array<"open" | "error"> = [];
  static sendHook: ((payload: unknown) => void) | undefined;

  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: (() => void) | null = null;
  send = vi.fn((payload: unknown) => {
    FakeWebSocket.sendHook?.(payload);
  });
  close = vi.fn();

  constructor(public url: string) {
    FakeWebSocket.instances.push(this);
    const behavior = FakeWebSocket.behaviors.shift() ?? "open";
    queueMicrotask(() => {
      if (behavior === "error") this.onerror?.();
      else this.onopen?.();
    });
  }

  emit(value: unknown): void {
    this.onmessage?.({ data: typeof value === "string" ? value : JSON.stringify(value) });
  }

  closeFromServer(): void {
    this.onclose?.();
  }
}

function lastSocket(): FakeWebSocket {
  const socket = FakeWebSocket.instances.at(-1);
  if (!socket) throw new Error("missing socket instance");
  return socket;
}

async function flush(): Promise<void> {
  for (let index = 0; index < 8; index += 1) await Promise.resolve();
}

function sentFrame(socket: FakeWebSocket, index = -1) {
  const call = index < 0 ? socket.send.mock.calls.at(index) : socket.send.mock.calls[index];
  if (!call) throw new Error(`missing sent frame ${index}`);
  return JSON.parse(String(call[0])) as {
    id: string;
    method: string;
    params: unknown;
    options?: { stream?: boolean };
  };
}

beforeEach(() => {
  vi.useRealTimers();
  vi.resetModules();
  FakeWebSocket.instances = [];
  FakeWebSocket.behaviors = [];
  FakeWebSocket.sendHook = undefined;
  vi.stubGlobal("WebSocket", FakeWebSocket);
  h.invoke.mockReset();
  h.invoke.mockResolvedValue({ port: 3131, token: "secret token" });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("client transport", () => {
  it("handles endpoint retry, RPC frames, streams, send failures, and timeout", async () => {
    vi.useFakeTimers();
    h.invoke
      .mockResolvedValueOnce({ port: 0, token: "boot" })
      .mockRejectedValueOnce(new Error("not ready"))
      .mockResolvedValueOnce({ port: 3131, token: "old token" })
      .mockResolvedValue({ port: 4242, token: "secret token" });
    FakeWebSocket.behaviors = ["error", "open"];
    const { client } = await import("./client");
    const resync = vi.fn();
    const offResync = client.onResync(resync);

    await expect(client.rpc("before-ready")).rejects.toThrow("websocket server is not ready");
    expect(FakeWebSocket.instances).toHaveLength(0);
    await expect(client.rpc("not-ready")).rejects.toThrow("not ready");
    await expect(client.rpc("failed-dial")).rejects.toThrow("websocket connect failed");
    const result = client.rpc<string>("echo", { value: 1 });
    await flush();
    expect(h.invoke).toHaveBeenCalledTimes(4);
    expect(FakeWebSocket.instances.map((socket) => socket.url)).toEqual([
      "ws://127.0.0.1:3131/ws?token=old%20token",
      "ws://127.0.0.1:4242/ws?token=secret%20token",
    ]);
    const socket = lastSocket();
    const resultFrame = sentFrame(socket);
    expect(resultFrame).toMatchObject({ method: "echo", params: { value: 1 } });

    // 非字符串载荷（binary）与不合法 JSON 都被忽略
    socket.onmessage?.({ data: new Uint8Array([1]) });
    socket.emit("{not json");
    socket.emit({ id: "unknown", result: "ignored" });
    socket.emit({ stream: { id: "sys.resync", seq: 1 }, result: null });
    expect(resync).toHaveBeenCalledOnce();
    socket.emit({ id: resultFrame.id, result: "ok" });
    await expect(result).resolves.toBe("ok");

    const failure = client.rpc("fail");
    await flush();
    const errorFrame = sentFrame(socket);
    expect(errorFrame.params).toEqual({});
    socket.emit({ id: errorFrame.id, error: { code: -32603, message: "denied", data: { h: 1 } } });
    await expect(failure).rejects.toThrow("denied");
    // 错误帧的 code/data 随 Error 上抛：-32601 与 -32603 前端可区分（rewind 的 message 内嵌 JSON 不动）
    const { RpcError } = await import("./client");
    await expect(failure).rejects.toBeInstanceOf(RpcError);
    await expect(failure).rejects.toMatchObject({ code: -32603, data: { h: 1 } });
    expect(FakeWebSocket.instances).toHaveLength(2);

    offResync();
    socket.emit({ stream: { id: "sys.resync", seq: 2 }, result: null });
    expect(resync).toHaveBeenCalledOnce();

    const subscriptionValues: unknown[] = [];

    const offSubscription = client
      .stream<{ text?: string }>(["task.update", "llm.delta"])
      .filter((value) => typeof value.text === "string")
      .map((value) => value.text)
      .on((value) => subscriptionValues.push(value));
    await vi.waitFor(() => expect(socket.send).toHaveBeenCalled());
    const subscribe = sentFrame(socket);
    expect(subscribe).toMatchObject({
      method: "rpc.subscribe",
      params: { topics: ["task.update", "llm.delta"] },
      options: { stream: true },
    });
    socket.emit({ id: subscribe.id, result: { stream_id: "sub-1" } });
    await flush();

    socket.emit({
      stream: { id: "sub-new", seq: 1 },
      result: { topic: "llm.delta", payload: { text: "a" } },
    });
    socket.emit({
      stream: { id: "sub-new", seq: 2 },
      result: { topic: "other", payload: "ignored" },
    });
    // filter 负路径：无 text 字段的 payload 被派生流滤掉
    socket.emit({
      stream: { id: "sub-new", seq: 3 },
      result: { topic: "llm.delta", payload: { n: 1 } },
    });
    // run 流原始帧（无 {topic, payload} 包装）不进 sub 处理器
    socket.emit({ stream: { id: "run-1", seq: 4 }, result: 3 });
    expect(subscriptionValues).toEqual(["a"]);

    offSubscription();
    socket.emit({
      stream: { id: "sub-new", seq: 5 },
      result: { topic: "llm.delta", payload: { text: "b" } },
    });
    expect(subscriptionValues).toEqual(["a"]);

    await vi.waitFor(() => expect(socket.send).toHaveBeenCalledTimes(4));
    const unsubscribe = sentFrame(socket);
    expect(unsubscribe).toMatchObject({
      method: "rpc.unsubscribe",
      params: { stream_id: "sub-1" },
    });
    socket.emit({ id: unsubscribe.id, result: null });

    // 原生 send 同步抛错 -> Promise reject（旧 plugin 异步 reject 语义的等价转换）。
    // send 失败会 drop 连接，下一次 RPC 换新实例：失败注入挂在跨实例的 sendHook 上。
    FakeWebSocket.sendHook = () => {
      throw new Error("send failed");
    };
    await expect(client.rpc("send-error")).rejects.toThrow("send failed");
    FakeWebSocket.sendHook = () => {
      throw "closed";
    };
    await expect(client.rpc("send-string-error")).rejects.toThrow("closed");
    FakeWebSocket.sendHook = undefined;

    const timeout = client.rpc("slow");
    const timeoutAssertion = expect(timeout).rejects.toThrow("rpc timeout: slow");
    await flush();
    await vi.advanceTimersByTimeAsync(30_000);
    await timeoutAssertion;
  });

  it("supports cancellation before source readiness and absorbs source rejection", async () => {
    const { TopicStream } = await import("./client");
    let deliver: ((value: unknown) => void) | undefined;
    let resolveUnsub: ((unsub: () => void) => void) | undefined;
    const unsub = vi.fn();
    const values: unknown[] = [];
    const stream = new TopicStream(
      (handler) =>
        new Promise((resolve) => {
          deliver = handler;
          resolveUnsub = resolve;
        }),
    );
    const off = stream.on((value) => values.push(value));
    off();
    deliver?.("ignored");
    resolveUnsub?.(unsub);
    await flush();
    expect(values).toEqual([]);
    expect(unsub).toHaveBeenCalledOnce();

    const rejected = new TopicStream(() => Promise.reject(new Error("offline")));
    const rejectedOff = rejected.on(vi.fn());
    rejectedOff();
    await flush();
  });
});
