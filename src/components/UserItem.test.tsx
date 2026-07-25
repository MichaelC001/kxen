// UserItem：右键「编辑并重发」与铅笔同一编辑框入口（旧右键跳过编辑框直接原文重发）；
// 无 messageId 的乐观消息 rewind 禁用；图片 load 回调（宿主据此重钉底）。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";
import { closeMenu, menu } from "../lib/context-menu";
import type { MsgItem } from "../lib/items";
import UserItem from "./UserItem";

const base = (extra: Partial<MsgItem> = {}): MsgItem => ({
  kind: "msg",
  role: "user",
  content: "原文",
  messageId: "u1",
  ...extra,
});

function setup(
  itemProps: MsgItem,
  onEditResend: (t: string) => void = () => {},
  onImageLoad?: () => void,
) {
  return render(
    () => (
      <UserItem
        item={itemProps}
        onFork={() => {}}
        onEditResend={onEditResend}
        onRewind={() => {}}
        onRetry={() => {}}
        {...(onImageLoad ? { onImageLoad } : {})}
      />
    ),
    document.body,
  );
}

function openContextMenu() {
  const el = document.body.querySelector(".group");
  if (!el) throw new Error("UserItem 未渲染");
  el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 10 }));
  return menu()?.items ?? [];
}

afterEach(() => {
  closeMenu();
  document.body.innerHTML = "";
});

describe("编辑并重发入口一致", () => {
  it("右键进编辑框（预填原文）不直接重发；提交走编辑后文本", () => {
    const editResend = vi.fn();
    setup(base(), editResend);
    const edit = openContextMenu().find((i) => i.label === "编辑并重发");
    expect(edit).toBeTruthy();
    edit!.action();
    closeMenu();
    const ta = document.body.querySelector("textarea");
    expect(ta).toBeTruthy();
    expect(ta!.value).toBe("原文");
    expect(editResend).not.toHaveBeenCalled();

    ta!.value = "改过的文本";
    ta!.dispatchEvent(new InputEvent("input", { bubbles: true }));
    const submit = [...document.body.querySelectorAll("button")].find(
      (b) => b.textContent === "重发（开分支）",
    );
    submit!.click();
    expect(editResend).toHaveBeenCalledWith("改过的文本");
  });
});

describe("rewind 入口", () => {
  it("有 messageId：可用", () => {
    setup(base());
    const rewind = openContextMenu().find((i) => i.label === "回退到此处");
    expect(rewind?.disabled).toBe(false);
  });

  it("无 messageId（未持久化乐观消息）：禁用（点了只会报 missing message_id）", () => {
    setup(base({ messageId: undefined }));
    const rewind = openContextMenu().find((i) => i.label === "回退到此处");
    expect(rewind?.disabled).toBe(true);
  });
});

describe("图片附件", () => {
  it("图片 load 后回调 onImageLoad（异步解码撑高列表，宿主重钉底）", () => {
    const onImageLoad = vi.fn();
    setup(base({ images: [{ media_type: "image/png", data: "QUJD" }] }), () => {}, onImageLoad);
    const img = document.body.querySelector("img");
    expect(img).toBeTruthy();
    img!.dispatchEvent(new Event("load"));
    expect(onImageLoad).toHaveBeenCalledTimes(1);
  });
});
