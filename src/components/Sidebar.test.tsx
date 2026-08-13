import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { JSX } from "solid-js";
import { COMMAND_PALETTE_OPEN_EVENT } from "../lib/command-palette";

const h = vi.hoisted(() => ({
  initSessions: vi.fn(async () => {}),
  mountSessionEvents: vi.fn(() => vi.fn()),
  newSession: vi.fn(async () => {}),
  toggleTheme: vi.fn(),
}));

vi.mock("@solidjs/router", () => ({
  A: (props: { href: string; class?: string; children: JSX.Element; "aria-label"?: string }) => (
    <a href={props.href} class={props.class} aria-label={props["aria-label"]}>
      {props.children}
    </a>
  ),
}));
vi.mock("../lib/state", () => ({
  initSessions: h.initSessions,
  mountSessionEvents: h.mountSessionEvents,
  newSession: h.newSession,
}));
vi.mock("../lib/theme", () => ({ theme: () => "dark", toggleTheme: h.toggleTheme }));
vi.mock("../lib/runtime", () => ({ isTauri: () => false }));
vi.mock("./SessionTree", () => ({ default: () => <div data-testid="session-tree">项目树</div> }));

import Sidebar from "./Sidebar";

afterEach(() => {
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

describe("Sidebar 信息架构", () => {
  it("按搜索、一级导航、项目区和底部设置组织，不再把入口挤进横排 footer", async () => {
    const dispose = render(() => <Sidebar />, document.body);
    await vi.waitFor(() => expect(h.initSessions).toHaveBeenCalledTimes(1));

    const aside = document.querySelector("aside[aria-label='应用侧边栏']");
    const search = [...document.querySelectorAll("button")].find((button) =>
      button.textContent?.includes("搜索"),
    );
    const workspace = document.querySelector("a[href='/workspaces']");
    const bots = document.querySelector("a[href='/bots']");
    const projectTree = document.querySelector("[data-testid='session-tree']");
    const settings = document.querySelector("a[href='/settings']");

    expect(aside).toBeTruthy();
    expect(search).toBeTruthy();
    expect(workspace?.textContent).toContain("工作区");
    expect(bots?.textContent).toContain("Bots");
    expect(document.body.textContent).toContain("项目");
    expect(projectTree).toBeTruthy();
    expect(settings?.textContent).toContain("设置");
    expect(
      workspace!.compareDocumentPosition(projectTree!) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      projectTree!.compareDocumentPosition(settings!) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    dispose();
  });

  it("Search 打开共享 Command Palette，项目区加号创建会话", async () => {
    let requests = 0;
    const onOpen = () => requests++;
    window.addEventListener(COMMAND_PALETTE_OPEN_EVENT, onOpen);
    const dispose = render(() => <Sidebar />, document.body);

    const search = [...document.querySelectorAll<HTMLButtonElement>("button")].find((button) =>
      button.textContent?.includes("搜索"),
    );
    search?.click();
    expect(requests).toBe(1);

    document.querySelector<HTMLButtonElement>("button[aria-label='新建会话']")?.click();
    expect(h.newSession).toHaveBeenCalledTimes(1);

    window.removeEventListener(COMMAND_PALETTE_OPEN_EVENT, onOpen);
    dispose();
  });
});
