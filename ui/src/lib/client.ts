// JSON-RPC 3.0 客户端：单连接多路复用 + 流式 API（client.rpc / client.stream，RxJS 理念最小实现）。
import WebSocket from "@tauri-apps/plugin-websocket";
import { invoke } from "@tauri-apps/api/core";

export type Unsub = () => void;

const VERSION = "3.0" as const;

/** 最小流：then 订阅返回 unsub；filter/map 派生新流。 */
export class TopicStream<T = unknown> {
  constructor(private readonly source: (handler: (payload: unknown) => void) => Promise<Unsub>) {}

  then(cb: (value: T) => void): Unsub {
    let cancelled = false;
    const ready = this.source((payload) => {
      if (!cancelled) cb(payload as T);
    });
    return () => {
      cancelled = true;
      void ready.then((unsub) => unsub());
    };
  }

  filter(predicate: (value: T) => boolean): TopicStream<T> {
    return new TopicStream<T>((handler) => this.source((payload) => {
      if (predicate(payload as T)) handler(payload);
    }));
  }

  map<U>(project: (value: T) => U): TopicStream<U> {
    return new TopicStream<U>((handler) => this.source((payload) => handler(project(payload as T))));
  }
}

// ---------------- 协议帧 ----------------

interface RpcResponse {
  id?: string | number;
  resId?: string;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

interface StreamChunk {
  stream?: { id: string; seq: number; complete?: boolean };
  result?: unknown;
}

// ---------------- 连接管理（单连接 + 掉线重连 + 订阅恢复） ----------------

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

let socket: WebSocket | null = null;
let connecting: Promise<WebSocket> | null = null;
let portPromise: Promise<number> | null = null;
const pending = new Map<string, Pending>();
let seq = 0;

/** 活跃订阅（streamId -> topics），重连后恢复。 */
const subscriptions = new Map<string, string[]>();
const chunkHandlers = new Set<(chunk: StreamChunk) => void>();

function getPort(): Promise<number> {
  portPromise ??= invoke<number>("ws_port");
  return portPromise;
}

async function ensureConn(): Promise<WebSocket> {
  if (socket) return socket;
  connecting ??= (async () => {
    const port = await getPort();
    const ws = await WebSocket.connect(`ws://127.0.0.1:${port}/`);
    ws.addListener((arg) => {
      if (typeof arg.data !== "string") return;
      let msg: RpcResponse & StreamChunk;
      try {
        msg = JSON.parse(arg.data);
      } catch {
        return;
      }
      if (msg.stream?.id) {
        chunkHandlers.forEach((h) => h(msg));
        return;
      }
      if (msg.id !== undefined) {
        const entry = pending.get(String(msg.id));
        if (!entry) return;
        pending.delete(String(msg.id));
        clearTimeout(entry.timer);
        if (msg.error) {
          entry.reject(new Error(msg.error.message));
        } else {
          entry.resolve(msg.result);
        }
      }
    });
    // 掉线探测：heartbeat 失败即重连并恢复订阅
    const heartbeat = setInterval(() => {
      if (!socket) {
        clearInterval(heartbeat);
        return;
      }
      void client
        .rpc("rpc.heartbeat")
        .catch(() => {
          clearInterval(heartbeat);
          drop();
        });
    }, 15_000);
    socket = ws;
    return ws;
  })();
  try {
    return await connecting;
  } finally {
    connecting = null;
  }
}

function drop() {
  socket = null;
  for (const entry of pending.values()) {
    clearTimeout(entry.timer);
    entry.reject(new Error("connection lost"));
  }
  pending.clear();
  // 1s 后重连并恢复全部订阅
  setTimeout(() => {
    void ensureConn().then(async () => {
      for (const [streamId, topics] of subscriptions) {
        subscriptions.delete(streamId);
        await openSubscription(topics).catch(() => {});
      }
    });
  }, 1000);
}

async function call<T>(method: string, params?: unknown): Promise<T> {
  const ws = await ensureConn();
  const id = `${Date.now()}-${seq++}`;
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`rpc timeout: ${method}`));
    }, 30_000);
    pending.set(id, { resolve: resolve as (v: unknown) => void, reject, timer });
    ws.send(JSON.stringify({ jsonrpc: VERSION, id, method, params: params ?? {} })).catch((e) => {
      pending.delete(id);
      clearTimeout(timer);
      reject(e instanceof Error ? e : new Error(String(e)));
    });
  });
}

// ---------------- 订阅（rpc.subscribe -> sub 流） ----------------

async function openSubscription(topics: string[]): Promise<string> {
  const result = await call<{ stream_id: string }>("rpc.subscribe", { topics });
  subscriptions.set(result.stream_id, topics);
  return result.stream_id;
}

async function closeSubscription(streamId: string): Promise<void> {
  subscriptions.delete(streamId);
  await call("rpc.unsubscribe", { stream_id: streamId });
}

// ---------------- 对外：client 单例 ----------------

export const client = {
  /** client.rpc("goal.list").then(...) */
  rpc<T = unknown>(method: string, params?: unknown): Promise<T> {
    return call<T>(method, params);
  },

  /** client.stream(["llm.delta"]).then(cb)：sub 流 chunk 的 result（{topic, payload} 解包为 payload）。 */
  stream<T = unknown>(topics: string | string[]): TopicStream<T> {
    const list = Array.isArray(topics) ? topics : [topics];
    return new TopicStream<T>(async (handler) => {
      const streamId = await openSubscription(list);
      const onChunk = (chunk: StreamChunk) => {
        if (chunk.stream?.id !== streamId) return;
        const result = chunk.result as { payload?: unknown } | undefined;
        handler(result?.payload ?? chunk.result);
      };
      chunkHandlers.add(onChunk);
      return () => {
        chunkHandlers.delete(onChunk);
        void closeSubscription(streamId);
      };
    });
  },

  /** run 流直读（send_message 返回的 stream_id 的 chunk 流）。 */
  runStream<T = unknown>(streamId: string): TopicStream<T> {
    return new TopicStream<T>(async (handler) => {
      const onChunk = (chunk: StreamChunk) => {
        if (chunk.stream?.id === streamId) handler(chunk.result);
      };
      chunkHandlers.add(onChunk);
      return () => {
        chunkHandlers.delete(onChunk);
      };
    });
  },
};
