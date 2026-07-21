// RPC 通道（/rpc）：请求-响应，id 关联，支持并发调用。
import WebSocket from "@tauri-apps/plugin-websocket";

declare global {
  interface Window {
    __KXEN_WS_PORT__?: number;
  }
}

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

let ws: WebSocket | null = null;
let connecting: Promise<WebSocket> | null = null;
const pending = new Map<string, Pending>();
let seq = 0;

async function conn(): Promise<WebSocket> {
  if (ws) return ws;
  if (connecting) return connecting;
  connecting = (async () => {
    const port = window.__KXEN_WS_PORT__;
    if (!port) throw new Error("ws port not injected");
    const socket = await WebSocket.connect(`ws://127.0.0.1:${port}/rpc`);
    socket.addListener((arg) => {
      try {
        const msg = typeof arg.data === "string" ? JSON.parse(arg.data) : arg.data;
        const entry = pending.get(msg.id);
        if (!entry) return;
        pending.delete(msg.id);
        clearTimeout(entry.timer);
        if (msg.ok) {
          entry.resolve(msg.result);
        } else {
          entry.reject(new Error(msg.error ?? "rpc error"));
        }
      } catch {
        // 非 JSON 帧忽略
      }
    });
    ws = socket;
    return socket;
  })();
  return connecting;
}

export async function rpc<T = unknown>(
  method: string,
  params?: unknown,
  timeoutMs = 15_000,
): Promise<T> {
  const socket = await conn();
  const id = `r${++seq}`;
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`rpc timeout: ${method}`));
    }, timeoutMs);
    pending.set(id, {
      resolve: (v) => resolve(v as T),
      reject,
      timer,
    });
    void socket.send(JSON.stringify({ id, method, params: params ?? null })).catch((e) => {
      pending.delete(id);
      clearTimeout(timer);
      reject(e instanceof Error ? e : new Error(String(e)));
    });
  });
}
