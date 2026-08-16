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

/** 自定义端点的 per-request query 参数编辑行（Azure OpenAI 的 api-version 等）。 */
export interface QueryParamRow {
  key: string;
  value: string;
}

export const [queryParamRows, setQueryParamRows] = createSignal<QueryParamRow[]>([]);

/**
 * 汇总编辑行为 RPC record：全空行忽略，同名键后者覆盖前者（与后端 BTreeMap 语义一致）。
 * 键值规则与后端 custom_provider::validate_query_params 一致：键非空无空白、值无控制字符。
 */
export function collectQueryParams(): { params?: Record<string, string>; error?: string } {
  const params: Record<string, string> = {};
  for (const row of queryParamRows()) {
    if (!row.key && !row.value) continue;
    if (!row.key.trim()) return { error: "query 参数的键不能为空" };
    if (/\s/.test(row.key)) return { error: `query 参数键「${row.key}」不能含空白字符` };
    // eslint-disable-next-line no-control-regex -- 控制字符正是要拦截的目标
    if (/[\x00-\x1f\x7f]/.test(row.value))
      return { error: `query 参数「${row.key}」的值不能含控制字符` };
    params[row.key] = row.value;
  }
  return Object.keys(params).length > 0 ? { params } : {};
}

export const resetAccountForm = () => {
  setName("");
  setToken("");
  setBaseUrl("");
  setModels("");
  setRegion("");
  setQueryParamRows([]);
};

// 名字进凭证键（provider:名）与 custom_providers 表键：冒号撕裂账号键解析，空白不可读
export const ACCOUNT_NAME_BAD = /[:：\s]/;

/**
 * 实际提交的凭证形态：anthropic 官方 API key（sk-ant-api…）不是 OAuth 凭证，
 * 在订阅 tab 手贴时按 api 存储，分发走 x-api-key 直连（OAuth 契约不适用）。
 */
export function effectiveSubmitKind(
  kind: AccountKind,
  provider: string,
  token: string,
): "oauth" | "api" {
  if (kind === "apikey") return "api";
  if (provider === "anthropic" && token.trim().startsWith("sk-ant-api")) return "api";
  return "oauth";
}

/** OAuth JSON 粘贴 -> 拆出 access/refresh/expires；`{` 开头但 JSON 损坏是明确错误，不静默降级。 */
export function parseAccountToken(
  kind: AccountKind,
  raw: string,
): {
  access: string;
  refresh: string;
  expires: number;
  error?: string; // 解析失败：调用方必须中止，不得当裸 token 用
  warning?: string; // 可继续但需提示用户
} {
  const access = raw.trim();
  if (kind !== "oauth" || !access) return { access, refresh: "", expires: 0 };
  // 缺 refresh_token 的凭证过期后无法自动续期，只能再贴一次
  const noRefresh = "缺少 refresh_token，token 过期后需重新手动粘贴";
  if (!access.startsWith("{")) return { access, refresh: "", expires: 0, warning: noRefresh };
  try {
    const j = JSON.parse(access) as {
      access_token?: string;
      refresh_token?: string;
      expires_at?: number;
    };
    const refresh = j.refresh_token ?? "";
    return {
      access: j.access_token ?? access,
      refresh,
      expires: j.expires_at ?? 0,
      ...(refresh ? {} : { warning: noRefresh }),
    };
  } catch (e) {
    return {
      access: "",
      refresh: "",
      expires: 0,
      error: `JSON 解析失败：${e instanceof Error ? e.message : String(e)}`,
    };
  }
}
