// AddAccountPanel 自定义提供商 query 参数回归：键值对行可增删，保存/探测均携带；
// 键含空白、只有值没有键时拒绝提交（规则与后端 custom_provider::validate_query_params 一致）。
import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderInfo } from "../../lib/provider";

const h = vi.hoisted(() => ({
  list: vi.fn(async () => [] as ProviderInfo[]),
  addCustom: vi.fn(async () => {}),
  probeModels: vi.fn(async () => ({ models: ["m1", "m2"] })),
}));

vi.mock("../../lib/provider", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/provider")>();
  return {
    ...orig,
    providerList: h.list,
    addCustomProvider: h.addCustom,
    probeModels: h.probeModels,
  };
});

import AddAccountPanel from "./AddAccountPanel";
import {
  queryParamRows,
  resetAccountForm,
  setBaseUrl,
  setKind,
  setModels,
  setName,
  setQueryParamRows,
  setToken,
} from "./add-account-form";

function btnByText(text: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent === text,
  );
  if (!found) throw new Error(`button not found: ${text}`);
  return found;
}

function rowInputs(placeholderPrefix: string): HTMLInputElement[] {
  return [...document.body.querySelectorAll<HTMLInputElement>("input")].filter((i) =>
    i.placeholder.startsWith(placeholderPrefix),
  );
}

function fillCustomForm() {
  setKind("custom");
  setName("azure");
  setBaseUrl("https://myres.openai.azure.com/openai/deployments/gpt-4o");
  setModels("gpt-4o");
  setToken("azure-key");
}

beforeEach(() => {
  resetAccountForm();
  setKind("custom");
});

afterEach(() => {
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

describe("AddAccountPanel query 参数", () => {
  it("键值对行可增删，保存时随 addCustomProvider 提交", async () => {
    const done = vi.fn();
    fillCustomForm();
    const dispose = render(() => <AddAccountPanel onDone={done} />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("query 参数"));
    expect(rowInputs("键")).toHaveLength(0);

    btnByText("添加参数").click();
    await vi.waitFor(() => expect(rowInputs("键")).toHaveLength(1));
    btnByText("添加参数").click();
    await vi.waitFor(() => expect(rowInputs("键")).toHaveLength(2));

    // 第二行全空（提交时忽略），第一行填 Azure api-version
    // （For 按行对象 key 化，每次输入都会重建该行 DOM，需重新查询后再填下一个框）
    const keyInput = rowInputs("键")[0]!;
    keyInput.value = "api-version";
    keyInput.dispatchEvent(new Event("input", { bubbles: true }));
    const valueInput = rowInputs("值")[0]!;
    valueInput.value = "2025-01-01-preview";
    valueInput.dispatchEvent(new Event("input", { bubbles: true }));
    btnByText("保存").click();

    await vi.waitFor(() =>
      expect(h.addCustom).toHaveBeenCalledWith(
        "azure",
        "https://myres.openai.azure.com/openai/deployments/gpt-4o",
        "azure-key",
        ["gpt-4o"],
        "openai",
        ["text"],
        { "api-version": "2025-01-01-preview" },
      ),
    );
    expect(done).toHaveBeenCalledWith("自定义提供商 azure 已添加");
    expect(queryParamRows()).toEqual([]); // 保存成功后表单重置
    dispose();
  });

  it("删除行按钮移除对应行", async () => {
    fillCustomForm();
    setQueryParamRows([
      { key: "a", value: "1" },
      { key: "b", value: "2" },
    ]);
    const dispose = render(() => <AddAccountPanel onDone={() => {}} />, document.body);
    await vi.waitFor(() => expect(rowInputs("键")).toHaveLength(2));

    const removeButtons = [
      ...document.body.querySelectorAll<HTMLButtonElement>("button[title='删除该参数']"),
    ];
    removeButtons[0]!.click();
    await vi.waitFor(() => expect(queryParamRows()).toEqual([{ key: "b", value: "2" }]));
    dispose();
  });

  it("键含空白或只有值没有键：拒绝保存且不发 RPC", async () => {
    fillCustomForm();
    setQueryParamRows([{ key: "api version", value: "v1" }]);
    const dispose = render(() => <AddAccountPanel onDone={() => {}} />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("query 参数"));

    btnByText("保存").click();
    await vi.waitFor(() => expect(document.body.textContent).toContain("不能含空白字符"));
    expect(h.addCustom).not.toHaveBeenCalled();

    setQueryParamRows([{ key: "", value: "v1" }]);
    btnByText("保存").click();
    await vi.waitFor(() => expect(document.body.textContent).toContain("键不能为空"));
    expect(h.addCustom).not.toHaveBeenCalled();
    dispose();
  });

  it("「测试连接并拉取模型」携带 query 参数；参数非法时不发 RPC", async () => {
    fillCustomForm();
    setQueryParamRows([{ key: "api-version", value: "2025-01-01-preview" }]);
    const dispose = render(() => <AddAccountPanel onDone={() => {}} />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("query 参数"));

    btnByText("测试连接并拉取模型").click();
    await vi.waitFor(() =>
      expect(h.probeModels).toHaveBeenCalledWith(
        "https://myres.openai.azure.com/openai/deployments/gpt-4o",
        "azure-key",
        "openai",
        { "api-version": "2025-01-01-preview" },
      ),
    );
    await vi.waitFor(() => expect(document.body.textContent).toContain("已拉取 2 个模型"));

    h.probeModels.mockClear();
    setQueryParamRows([{ key: "api version", value: "v1" }]);
    btnByText("测试连接并拉取模型").click();
    await vi.waitFor(() => expect(document.body.textContent).toContain("不能含空白字符"));
    expect(h.probeModels).not.toHaveBeenCalled();
    dispose();
  });
});
