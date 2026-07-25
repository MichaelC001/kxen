// 添加账号面板的模块级表单状态：面板随设置分区切换卸载重建，挂模块级 signal 半填表单不丢
// （成功保存才重置；kind/provider/protocol/caps 保存后也保留，连续添加同类账号是常态）。
import { createSignal } from "solid-js";

export type AccountKind = "oauth" | "apikey" | "custom";

export const [kind, setKind] = createSignal<AccountKind>("oauth");
export const [provider, setProvider] = createSignal("anthropic");
export const [region, setRegion] = createSignal("");
export const [name, setName] = createSignal("");
export const [token, setToken] = createSignal("");
export const [baseUrl, setBaseUrl] = createSignal("");
export const [models, setModels] = createSignal("");
export const [protocol, setProtocol] = createSignal<"openai" | "anthropic">("openai");
export const [caps, setCaps] = createSignal<string[]>(["text"]);

export const resetAccountForm = () => {
  setName("");
  setToken("");
  setBaseUrl("");
  setModels("");
  setRegion("");
};

// 名字进凭证键（provider:名）与 custom_providers 表键：冒号撕裂账号键解析，空白不可读
export const ACCOUNT_NAME_BAD = /[:：\s]/;

/** OAuth JSON 粘贴 -> 拆出 access/refresh/expires；非 JSON 按裸 token 处理。 */
export function parseAccountToken(
  kind: AccountKind,
  raw: string,
): {
  access: string;
  refresh: string;
  expires: number;
} {
  let access = raw.trim();
  let refresh = "";
  let expires = 0;
  if (kind === "oauth" && access.startsWith("{")) {
    try {
      const j = JSON.parse(access) as {
        access_token?: string;
        refresh_token?: string;
        expires_at?: number;
      };
      access = j.access_token ?? access;
      refresh = j.refresh_token ?? "";
      expires = j.expires_at ?? 0;
    } catch {
      /* 按裸 token 处理 */
    }
  }
  return { access, refresh, expires };
}
