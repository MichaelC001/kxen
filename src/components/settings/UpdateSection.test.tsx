import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  currentVersion: vi.fn(async () => "0.1.0"),
  checkForUpdate: vi.fn(),
  installUpdate: vi.fn(async () => {}),
}));

vi.mock("../../lib/updater", () => h);

import UpdateSection from "./UpdateSection";

function button(text: string): HTMLButtonElement {
  const found = [...document.querySelectorAll<HTMLButtonElement>("button")].find(
    (candidate) => candidate.textContent === text,
  );
  if (!found) throw new Error(`button not found: ${text}`);
  return found;
}

beforeEach(() => {
  h.currentVersion.mockResolvedValue("0.1.0");
  h.checkForUpdate.mockReset();
  h.installUpdate.mockReset();
  h.installUpdate.mockResolvedValue();
});

afterEach(() => {
  document.body.innerHTML = "";
});

describe("UpdateSection", () => {
  it("无更新时显示最新状态", async () => {
    h.checkForUpdate.mockResolvedValue(null);
    const dispose = render(() => <UpdateSection />, document.body);

    button("检查更新").click();

    await vi.waitFor(() => expect(document.body.textContent).toContain("当前已是最新版本"));
    expect(document.body.textContent).toContain("当前版本 0.1.0");
    dispose();
  });

  it("发现更新后安装并防止重复操作", async () => {
    const update = { version: "0.1.1" };
    h.checkForUpdate.mockResolvedValue(update);
    const dispose = render(() => <UpdateSection />, document.body);

    button("检查更新").click();
    await vi.waitFor(() => expect(document.body.textContent).toContain("发现版本 0.1.1"));

    button("下载并安装").click();
    await vi.waitFor(() => expect(h.installUpdate).toHaveBeenCalledWith(update));
    expect(button("处理中").disabled).toBe(true);
    dispose();
  });

  it("检查失败时显示原因并恢复按钮", async () => {
    h.checkForUpdate.mockRejectedValue(new Error("offline"));
    const dispose = render(() => <UpdateSection />, document.body);

    button("检查更新").click();

    await vi.waitFor(() => expect(document.body.textContent).toContain("检查失败：offline"));
    expect(button("检查更新").disabled).toBe(false);
    dispose();
  });
});
