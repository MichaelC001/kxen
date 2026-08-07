// WebTokenGuide：缺 token 引导页明示获取方式（服务端启动日志 / 托盘复制链接），不是空白或加载失败。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it } from "vitest";
import WebTokenGuide from "./WebTokenGuide";

afterEach(() => {
  document.body.innerHTML = "";
});

describe("WebTokenGuide", () => {
  it("渲染引导说明：需要带 token 的链接 + 获取渠道", () => {
    const dispose = render(() => <WebTokenGuide />, document.body);
    expect(document.body.textContent).toContain("token");
    expect(document.body.textContent).toContain("启动日志");
    expect(document.body.textContent).toContain("复制访问链接");
    dispose();
  });
});
