// 添加账号面板：三类入口（订阅 OAuth / API Key / 自定义类型提供商），先选型再填字段。
// provider 与区域下拉来自后端 provider.list（registry 是唯一真相源，前端不硬编码）。
import { createSignal, For, onMount, Show } from "solid-js";
import {
  addCustomProvider,
  importAccount,
  providerList,
  type ProviderInfo,
} from "../../lib/provider";

type Kind = "oauth" | "apikey" | "custom";

const KINDS: { id: Kind; label: string; detail: string }[] = [
  {
    id: "oauth",
    label: "订阅 OAuth",
    detail: "Claude/ChatGPT/Grok/Kimi 订阅（OAuth JSON 或 access token）",
  },
  {
    id: "apikey",
    label: "API Key",
    detail: "官方平台 key（DeepSeek / 月之暗面 / 智谱 / 通义 / Mistral / Groq / Gemini 等）",
  },
  { id: "custom", label: "自定义提供商", detail: "OpenAI / Anthropic 兼容端点（中转、自部署）" },
];

const CAPS = ["text", "vision", "audio"];

export default function AddAccountPanel(props: { onDone: (msg: string) => void }) {
  const [kind, setKind] = createSignal<Kind>("oauth");
  const [providers, setProviders] = createSignal<ProviderInfo[]>([]);
  const [provider, setProvider] = createSignal("anthropic");
  const [region, setRegion] = createSignal("");
  const [name, setName] = createSignal("");
  const [token, setToken] = createSignal("");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [models, setModels] = createSignal("");
  const [protocol, setProtocol] = createSignal<"openai" | "anthropic">("openai");
  const [caps, setCaps] = createSignal<string[]>(["text"]);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  onMount(async () => {
    const list = await providerList().catch(() => [] as ProviderInfo[]);
    setProviders(list);
    if (list.length > 0 && !list.some((p) => p.key === provider())) {
      setProvider(list[0]!.key);
    }
  });

  const spec = () => providers().find((p) => p.key === provider());
  const regions = () => spec()?.regions ?? [];

  const toggleCap = (c: string) =>
    setCaps((prev) => (prev.includes(c) ? prev.filter((x) => x !== c) : [...prev, c]));

  const submit = async () => {
    setBusy(true);
    setError("");
    try {
      if (kind() === "custom") {
        const list = models()
          .split(/[,，\s]+/)
          .filter(Boolean);
        if (!name().trim() || !baseUrl().trim() || list.length === 0 || !token().trim()) {
          setError("名称 / base_url / 模型 / key 均必填");
          return;
        }
        await addCustomProvider(
          name().trim(),
          baseUrl().trim(),
          token().trim(),
          list,
          protocol(),
          caps(),
        );
        props.onDone(`自定义提供商 ${name()} 已添加`);
        return;
      }
      if (!name().trim() || !token().trim()) {
        setError("账号名与凭证均必填");
        return;
      }
      let access = token().trim();
      let refresh = "";
      let expires = 0;
      if (kind() === "oauth" && access.startsWith("{")) {
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
      await importAccount(
        provider(),
        name().trim(),
        access,
        kind() === "apikey" ? "api" : "oauth",
        refresh,
        expires,
        // 多区域厂商才带 region；单区域/空选择交给后端缺省
        regions().length > 1 && region() ? region() : undefined,
      );
      props.onDone(`账号 ${provider()}:${name()} 已添加`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-3 space-y-2.5">
      <div class="flex gap-1.5">
        <For each={KINDS}>
          {(k) => (
            <button
              class="pressable px-2.5 py-1 rounded-md text-xs border"
              classList={{
                "border-[var(--accent)] text-[var(--accent-hover)]": kind() === k.id,
                "border-[var(--border)] text-[var(--text-dim)]": kind() !== k.id,
              }}
              title={k.detail}
              onClick={() => setKind(k.id)}
            >
              {k.label}
            </button>
          )}
        </For>
      </div>
      <div class="text-2xs text-[var(--text-faint)]">
        {KINDS.find((k) => k.id === kind())?.detail}
      </div>

      <Show when={kind() !== "custom"}>
        <div class="flex gap-2">
          <select
            class="bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-xs text-[var(--text-dim)]"
            value={provider()}
            onChange={(e) => {
              setProvider(e.currentTarget.value);
              setRegion("");
            }}
          >
            <For each={providers()}>{(p) => <option value={p.key}>{p.display}</option>}</For>
          </select>
          <Show when={regions().length > 1}>
            <select
              class="bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-xs text-[var(--text-dim)]"
              title="运营区域（账号凭证只对该区域端点有效）"
              value={region() || regions()[0]?.key}
              onChange={(e) => setRegion(e.currentTarget.value)}
            >
              <For each={regions()}>
                {(r) => <option value={r.key}>{`${spec()?.display} ${r.display}`}</option>}
              </For>
            </select>
          </Show>
          <input
            class="flex-1 bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs"
            placeholder="账号名（如 work / personal）"
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
          />
        </div>
        <textarea
          class="w-full bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs font-mono h-14"
          placeholder={
            kind() === "oauth"
              ? "OAuth JSON（access_token/refresh_token/expires_at）或裸 access token"
              : "sk-... API key"
          }
          value={token()}
          onInput={(e) => setToken(e.currentTarget.value)}
        />
      </Show>

      <Show when={kind() === "custom"}>
        <div class="flex gap-2">
          <input
            class="flex-1 bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs"
            placeholder="提供商名（英文，如 my-relay）"
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
          />
          <select
            class="bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-xs text-[var(--text-dim)]"
            value={protocol()}
            onChange={(e) => setProtocol(e.currentTarget.value as "openai" | "anthropic")}
          >
            <option value="openai">openai 协议</option>
            <option value="anthropic">anthropic 协议</option>
          </select>
        </div>
        <input
          class="w-full bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs font-mono"
          placeholder="base_url（API 根，如 https://relay.example.com/v1）"
          value={baseUrl()}
          onInput={(e) => setBaseUrl(e.currentTarget.value)}
        />
        <input
          class="w-full bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs font-mono"
          placeholder="模型清单（逗号分隔，如 gpt-4o, claude-sonnet-4-5）"
          value={models()}
          onInput={(e) => setModels(e.currentTarget.value)}
        />
        <input
          type="password"
          class="w-full bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs font-mono"
          placeholder="api key（存本机 auth.json，0600）"
          value={token()}
          onInput={(e) => setToken(e.currentTarget.value)}
        />
        <div class="flex items-center gap-3 text-xs text-[var(--text-dim)]">
          能力：
          <For each={CAPS}>
            {(c) => (
              <label class="flex items-center gap-1 cursor-pointer">
                <input type="checkbox" checked={caps().includes(c)} onChange={() => toggleCap(c)} />
                {c}
              </label>
            )}
          </For>
          <span class="text-2xs text-[var(--text-faint)]">audio 可用于语音转写引擎</span>
        </div>
      </Show>

      <Show when={error()}>
        <div class="text-xs text-[var(--err)]">{error()}</div>
      </Show>
      <button
        class="pressable px-3 py-1 rounded-md text-xs border border-[var(--border)] disabled:opacity-40"
        disabled={busy()}
        onClick={() => void submit()}
      >
        {busy() ? "保存中…" : "保存"}
      </button>
    </div>
  );
}
