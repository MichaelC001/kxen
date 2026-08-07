// client-endpoint 双分支：Tauri 走 invoke 拼 127.0.0.1 URL；
// web 走同源 /ws，token URL -> sessionStorage -> replaceState 抹除，缺失 reject MissingWebTokenError。
// 浏览器模式 resetModules 不刷新模块：endpointPromise 缓存用 resetEndpoint() 逐用例清。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: h.invoke }));

import { getEndpoint, MissingWebTokenError, resetEndpoint } from "./client-endpoint";

const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
const STORAGE_KEY = "kxen.ws-token";
let savedInternals: unknown;
let savedUrl: string;

beforeEach(() => {
  savedInternals = w.__TAURI_INTERNALS__;
  savedUrl = window.location.href;
  resetEndpoint();
  h.invoke.mockReset();
});

afterEach(() => {
  w.__TAURI_INTERNALS__ = savedInternals;
  history.replaceState(null, "", savedUrl);
  sessionStorage.removeItem(STORAGE_KEY);
  resetEndpoint();
});

describe("client-endpoint Tauri 分支", () => {
  it("invoke 拿端口 token，拼 127.0.0.1 实际端口 URL", async () => {
    h.invoke.mockResolvedValue({ port: 7824, token: "secret token" });
    await expect(getEndpoint()).resolves.toEqual({
      url: "ws://127.0.0.1:7824/ws?token=secret%20token",
    });
    expect(h.invoke).toHaveBeenCalledWith("ws_port");
  });

  it("端口非法 / token 空都 reject（不拼出坏 URL）", async () => {
    h.invoke.mockResolvedValue({ port: 0, token: "t" });
    await expect(getEndpoint()).rejects.toThrow("websocket server is not ready");

    resetEndpoint();
    h.invoke.mockResolvedValue({ port: 7824, token: "" });
    await expect(getEndpoint()).rejects.toThrow("websocket endpoint token is unavailable");
  });
});

describe("client-endpoint web 分支", () => {
  beforeEach(() => {
    delete w.__TAURI_INTERNALS__;
  });

  it("URL ?token= 读入 sessionStorage，replaceState 抹除地址栏，URL 同源 /ws", async () => {
    history.replaceState(null, "", "?token=abc 123");
    const endpoint = await getEndpoint();
    expect(endpoint.url).toBe(`ws://${window.location.host}/ws?token=abc%20123`);
    expect(sessionStorage.getItem(STORAGE_KEY)).toBe("abc 123");
    expect(window.location.search).not.toContain("token");
  });

  it("无 URL token 时复用 sessionStorage（刷新后免链接）", async () => {
    sessionStorage.setItem(STORAGE_KEY, "reused");
    await expect(getEndpoint()).resolves.toEqual({
      url: `ws://${window.location.host}/ws?token=reused`,
    });
  });

  it("token 缺失 reject MissingWebTokenError（引导页判定依据，不静默挂起）", async () => {
    await expect(getEndpoint()).rejects.toBeInstanceOf(MissingWebTokenError);
  });

  it("http 页面给 ws 协议（https->wss 单行协议判断）", async () => {
    sessionStorage.setItem(STORAGE_KEY, "t");
    const endpoint = await getEndpoint();
    expect(endpoint.url.startsWith("ws://")).toBe(true);
  });
});
