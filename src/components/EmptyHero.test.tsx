// EmptyHero web 模式：「打开项目目录」卡换路径文本输入（浏览器无原生目录选择器）。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { userEvent } from "@vitest/browser/context";

const h = vi.hoisted(() => ({
  add: vi.fn<(path: string) => Promise<boolean>>(),
  openDialog: vi.fn<() => Promise<boolean>>(),
}));

vi.mock("../lib/open-project", () => ({
  addProjectDir: h.add,
  openProjectDir: h.openDialog,
}));

import EmptyHero from "./EmptyHero";

const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
let saved: unknown;

beforeEach(() => {
  h.add.mockReset().mockResolvedValue(true);
  h.openDialog.mockReset().mockResolvedValue(true);
});

afterEach(() => {
  w.__TAURI_INTERNALS__ = saved;
  document.body.innerHTML = "";
});

describe("EmptyHero web 模式", () => {
  it("点卡展开绝对路径输入，提交走 addProjectDir 文本路径", async () => {
    saved = w.__TAURI_INTERNALS__;
    delete w.__TAURI_INTERNALS__;
    const dispose = render(() => <EmptyHero />, document.body);
    const card = [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) =>
      b.textContent?.includes("打开项目目录"),
    )!;
    expect(card.textContent).toContain("绝对路径");
    await userEvent.click(card);

    const input = document.querySelector<HTMLInputElement>("input")!;
    await userEvent.fill(input, "/srv/project");
    await userEvent.keyboard("{Enter}");
    await vi.waitFor(() => expect(h.add).toHaveBeenCalledWith("/srv/project"));
    expect(h.openDialog).not.toHaveBeenCalled();
    dispose();
  });

  it("Escape 取消输入回到卡片", async () => {
    saved = w.__TAURI_INTERNALS__;
    delete w.__TAURI_INTERNALS__;
    const dispose = render(() => <EmptyHero />, document.body);
    const card = [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) =>
      b.textContent?.includes("打开项目目录"),
    )!;
    await userEvent.click(card);
    expect(document.querySelector("input")).not.toBeNull();
    await userEvent.keyboard("{Escape}");
    expect(document.querySelector("input")).toBeNull();
    expect(h.add).not.toHaveBeenCalled();
    dispose();
  });

  it("Tauri 模式点卡直接走原生选择器", async () => {
    saved = w.__TAURI_INTERNALS__;
    const dispose = render(() => <EmptyHero />, document.body);
    const card = [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) =>
      b.textContent?.includes("打开项目目录"),
    )!;
    await userEvent.click(card);
    await vi.waitFor(() => expect(h.openDialog).toHaveBeenCalledTimes(1));
    expect(document.querySelector("input")).toBeNull();
    dispose();
  });
});
