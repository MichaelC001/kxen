// WS 端点解析双分支：
// - Tauri webview：invoke ws_port 拿实际端口 + token，指向 127.0.0.1 内嵌服务；
// - 纯浏览器：同源 /ws，token 从 URL ?token= 一次性投递（读入 sessionStorage 后抹除地址栏），
//   再次加载从 sessionStorage 取；缺失时 reject MissingWebTokenError，由 main.tsx 出引导页。
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./runtime";

export interface WsEndpoint {
  /** 完整 ws(s) URL（已含 token query）。 */
  url: string;
}

const TOKEN_QUERY = "token";
const TOKEN_STORAGE_KEY = "kxen.ws-token";

/** web 模式缺 token 的显式错误：调用方据此停在引导页，不伪装成网络抖动重试。 */
export class MissingWebTokenError extends Error {
  constructor() {
    super("web 模式缺少访问 token：需要带 ?token= 的链接访问");
    this.name = "MissingWebTokenError";
  }
}

/**
 * web 模式 token 解析：URL ?token= -> sessionStorage -> history.replaceState 抹除；
 * 无 URL token 时读 sessionStorage（刷新/再次打开复用）。均无返回 null。
 */
export function resolveWebToken(): string | null {
  const fromUrl = new URLSearchParams(window.location.search).get(TOKEN_QUERY);
  if (fromUrl) {
    try {
      sessionStorage.setItem(TOKEN_STORAGE_KEY, fromUrl);
    } catch {
      // storage 被禁用（隐私模式等）：本会话仍用返回值连接，但刷新后按缺失处理
    }
    // token 不留在地址栏：防泄漏到历史记录、截图与转发
    const url = new URL(window.location.href);
    url.searchParams.delete(TOKEN_QUERY);
    history.replaceState(null, "", url);
    return fromUrl;
  }
  try {
    return sessionStorage.getItem(TOKEN_STORAGE_KEY);
  } catch {
    return null;
  }
}

function webEndpoint(): WsEndpoint {
  const token = resolveWebToken();
  if (!token) throw new MissingWebTokenError();
  const scheme = window.location.protocol === "https:" ? "wss" : "ws";
  return { url: `${scheme}://${window.location.host}/ws?token=${encodeURIComponent(token)}` };
}

function tauriEndpoint(): Promise<WsEndpoint> {
  return invoke<{ port: number; token: string }>("ws_port").then((endpoint) => {
    if (!Number.isInteger(endpoint.port) || endpoint.port <= 0 || endpoint.port > 65_535)
      throw new Error("websocket server is not ready");
    if (typeof endpoint.token !== "string" || endpoint.token.length === 0)
      throw new Error("websocket endpoint token is unavailable");
    return {
      url: `ws://127.0.0.1:${endpoint.port}/ws?token=${encodeURIComponent(endpoint.token)}`,
    };
  });
}

let endpointPromise: Promise<WsEndpoint> | null = null;

export function getEndpoint(): Promise<WsEndpoint> {
  if (endpointPromise) return endpointPromise;
  const request = isTauri() ? tauriEndpoint() : Promise.resolve().then(webEndpoint);
  endpointPromise = request;
  void request.catch(() => resetEndpoint(request));
  return request;
}

export function resetEndpoint(expected?: Promise<WsEndpoint>): void {
  if (!expected || endpointPromise === expected) endpointPromise = null;
}
