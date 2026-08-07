// AttachMenu 原生对话框：文件/图片按钮的 open 参数与选中路径透传（取消不回调）。
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { userEvent } from "@vitest/browser/context";
import AttachMenu from "./AttachMenu";
import { flash } from "../../lib/flash";
import "../../styles.css";

const dialogMock = vi.hoisted(() => ({
  result: null as unknown,
  error: null as Error | null,
  calls: [] as Array<Record<string, unknown>>,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (opts: Record<string, unknown>) => {
    dialogMock.calls.push(opts);
    if (dialogMock.error) return Promise.reject(dialogMock.error);
    return Promise.resolve(dialogMock.result);
  },
}));

afterEach(() => {
  dialogMock.result = null;
  dialogMock.error = null;
  dialogMock.calls.length = 0;
  document.body.innerHTML = "";
  for (const message of flash.msgs()) flash.dismiss(message.id);
});

async function openMenuAndClick(label: string, onPaths: (paths: string[]) => void) {
  const host = document.createElement("div");
  host.style.cssText = "position:fixed;bottom:8px;left:8px;";
  document.body.append(host);
  const dispose = render(() => <AttachMenu onPaths={onPaths} />, host);
  await userEvent.click(host.querySelector<HTMLButtonElement>(".attach-btn")!);
  const row = [...host.querySelectorAll<HTMLButtonElement>(".popup-row")].find((b) =>
    b.textContent?.includes(label),
  )!;
  await userEvent.click(row);
  await new Promise((r) => setTimeout(r, 50));
  return () => {
    dispose();
    host.remove();
  };
}

describe("AttachMenu (webkit)", () => {
  it("选择文件：multiple 无过滤器，路径数组透传", async () => {
    dialogMock.result = ["/tmp/a.txt", "/tmp/b.md"];
    let got: string[] = [];
    const dispose = await openMenuAndClick("选择文件", (p) => (got = p));
    expect(dialogMock.calls).toHaveLength(1);
    expect(dialogMock.calls[0]).toMatchObject({ multiple: true });
    expect(dialogMock.calls[0]?.filters).toBeUndefined();
    expect(got).toEqual(["/tmp/a.txt", "/tmp/b.md"]);
    dispose();
  });

  it("选择图片：带图片扩展名过滤器，单字符串归一为数组", async () => {
    dialogMock.result = "/tmp/pic.png";
    let got: string[] = [];
    const dispose = await openMenuAndClick("选择图片", (p) => (got = p));
    const filters = dialogMock.calls[0]?.filters as Array<{ extensions: string[] }>;
    expect(filters[0]?.extensions).toContain("png");
    expect(filters[0]?.extensions).toContain("webp");
    expect(got).toEqual(["/tmp/pic.png"]);
    dispose();
  });

  it("取消选择不回调 onPaths", async () => {
    dialogMock.result = null;
    let called = 0;
    const dispose = await openMenuAndClick("选择文件", () => called++);
    expect(called).toBe(0);
    dispose();
  });

  it("目录选择器失败显示错误，不伪装成用户取消", async () => {
    dialogMock.error = new Error("dialog unavailable");
    let called = 0;
    const dispose = await openMenuAndClick("选择文件", () => called++);

    expect(called).toBe(0);
    expect(
      flash
        .msgs()
        .some((message) => message.kind === "err" && message.text.includes("dialog unavailable")),
    ).toBe(true);
    dispose();
  });

  it("左下角打开时完整留在 1280×800 viewport 内，resize 后关闭", async () => {
    const host = document.createElement("div");
    host.style.cssText = "position:fixed;bottom:8px;left:8px;";
    document.body.append(host);
    const dispose = render(() => <AttachMenu onPaths={() => {}} />, host);
    await userEvent.click(host.querySelector<HTMLButtonElement>(".attach-btn")!);

    const rect = host.querySelector(".composer-popup")!.getBoundingClientRect();
    expect([window.innerWidth, window.innerHeight]).toEqual([1280, 800]);
    expect(rect.left).toBeGreaterThanOrEqual(8);
    expect(rect.right).toBeLessThanOrEqual(window.innerWidth - 8);
    expect(rect.top).toBeGreaterThanOrEqual(8);
    expect(rect.bottom).toBeLessThanOrEqual(window.innerHeight - 8);

    window.dispatchEvent(new Event("resize"));
    expect(host.querySelector(".composer-popup")).toBeNull();
    dispose();
  });

  it("web 模式（无 __TAURI_INTERNALS__）走 file input：File 对象透传 onFiles，不调原生对话框", async () => {
    const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
    const saved = w.__TAURI_INTERNALS__;
    delete w.__TAURI_INTERNALS__;
    const host = document.createElement("div");
    host.style.cssText = "position:fixed;bottom:8px;left:8px;";
    document.body.append(host);
    let got: File[] = [];
    const dispose = render(
      () => <AttachMenu onPaths={() => {}} onFiles={(f) => (got = f)} />,
      host,
    );
    try {
      await userEvent.click(host.querySelector<HTMLButtonElement>(".attach-btn")!);
      const row = [...host.querySelectorAll<HTMLButtonElement>(".popup-row")].find((b) =>
        b.textContent?.includes("选择文件"),
      )!;
      await userEvent.click(row);
      const input = host.querySelector<HTMLInputElement>("input[type=file]")!;
      await userEvent.upload(input, [
        new File(["abc"], "note.txt", { type: "text/plain" }),
        new File(["def"], "b.md", { type: "text/markdown" }),
      ]);
      await vi.waitFor(() => expect(got.map((f) => f.name)).toEqual(["note.txt", "b.md"]));
      expect(dialogMock.calls).toHaveLength(0);
      // value 已清零：同名文件可连续再选
      expect(input.value).toBe("");
    } finally {
      w.__TAURI_INTERNALS__ = saved;
      dispose();
      host.remove();
    }
  });

  it("web 模式选择图片：file input 带图片扩展名 accept", async () => {
    const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
    const saved = w.__TAURI_INTERNALS__;
    delete w.__TAURI_INTERNALS__;
    const host = document.createElement("div");
    host.style.cssText = "position:fixed;bottom:8px;left:8px;";
    document.body.append(host);
    let got: File[] = [];
    const dispose = render(
      () => <AttachMenu onPaths={() => {}} onFiles={(f) => (got = f)} />,
      host,
    );
    try {
      await userEvent.click(host.querySelector<HTMLButtonElement>(".attach-btn")!);
      const row = [...host.querySelectorAll<HTMLButtonElement>(".popup-row")].find((b) =>
        b.textContent?.includes("选择图片"),
      )!;
      await userEvent.click(row);
      const input = host.querySelector<HTMLInputElement>("input[type=file]")!;
      expect(input.accept).toContain(".png");
      expect(input.accept).toContain(".webp");
      await userEvent.upload(input, [
        new File([new Uint8Array([1])], "p.png", { type: "image/png" }),
      ]);
      await vi.waitFor(() => expect(got.map((f) => f.name)).toEqual(["p.png"]));
      expect(dialogMock.calls).toHaveLength(0);
    } finally {
      w.__TAURI_INTERNALS__ = saved;
      dispose();
      host.remove();
    }
  });
});
