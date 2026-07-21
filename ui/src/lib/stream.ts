// Stream 通道（/stream）：订阅-推送，topic 过滤。
import WebSocket from "@tauri-apps/plugin-websocket";

declare global {
  interface Window {
    __KXEN_WS_PORT__?: number;
  }
}

type Handler = (topic: string, payload: unknown) => void;

let ws: WebSocket | null = null;
let connecting: Promise<WebSocket> | null = null;
const handlers = new Set<Handler>();

async function conn(): Promise<WebSocket> {
  if (ws) return ws;
  if (connecting) return connecting;
  connecting = (async () => {
    const port = window.__KXEN_WS_PORT__;
    if (!port) throw new Error("ws port not injected");
    const socket = await WebSocket.connect(`ws://127.0.0.1:${port}/stream`);
    socket.addListener((arg) => {
      try {
        const msg = typeof arg.data === "string" ? JSON.parse(arg.data) : arg.data;
        if (typeof msg.topic !== "string") return;
        handlers.forEach((h) => h(msg.topic, msg.payload));
      } catch {
        // 非 JSON 帧忽略
      }
    });
    ws = socket;
    return socket;
  })();
  return connecting;
}

/** 订阅 topic（追加），返回退订函数。 */
export async function subscribe(topics: string[], handler: Handler): Promise<() => void> {
  const socket = await conn();
  handlers.add(handler);
  await socket.send(JSON.stringify({ action: "subscribe", topics }));
  return async () => {
    handlers.delete(handler);
    await socket.send(JSON.stringify({ action: "unsubscribe", topics })).catch(() => {});
  };
}
