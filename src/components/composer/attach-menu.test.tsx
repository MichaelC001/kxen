// AttachMenu 原生对话框：文件/图片按钮的 open 参数与选中路径透传（取消不回调）。
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { userEvent } from "@vitest/browser/context";
import AttachMenu from "./AttachMenu";

const dialogMock = vi.hoisted(() => ({
  result: null as unknown,
  calls: [] as Array<Record<string, unknown>>,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (opts: Record<string, unknown>) => {
    dialogMock.calls.push(opts);
    return Promise.resolve(dialogMock.result);
  },
}));

afterEach(() => {
  dialogMock.result = null;
  dialogMock.calls.length = 0;
  document.body.innerHTML = "";
});

async function openMenuAndClick(label: string, onPaths: (paths: string[]) => void) {
  const dispose = render(() => <AttachMenu onPaths={onPaths} />, document.body);
  await userEvent.click(document.querySelector<HTMLButtonElement>(".attach-btn")!);
  const row = [...document.querySelectorAll<HTMLButtonElement>(".popup-row")].find((b) =>
    b.textContent?.includes(label),
  )!;
  await userEvent.click(row);
  await new Promise((r) => setTimeout(r, 50));
  return dispose;
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
});
