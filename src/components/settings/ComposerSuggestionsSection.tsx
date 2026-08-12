import { createSignal, For, onMount, Show } from "solid-js";
import { configGet, configSetComposerSuggestions, configSetEmbedding } from "../../lib/chat";
import { errText } from "../err-text";

type Flag = "enabled" | "semantic" | "llm";

export default function ComposerSuggestionsSection() {
  const [flags, setFlags] = createSignal({ enabled: true, semantic: false, llm: false });
  const [embedding, setEmbedding] = createSignal({ provider: "", model: "", baseUrl: "" });
  const [loaded, setLoaded] = createSignal(false);
  const [saving, setSaving] = createSignal("");
  const [error, setError] = createSignal("");

  const reload = async () => {
    try {
      const config = await configGet();
      setFlags({
        enabled: config.composer_suggestions?.enabled !== false,
        semantic: config.composer_suggestions?.semantic === true,
        llm: config.composer_suggestions?.llm === true,
      });
      setEmbedding({
        provider: config.embedding?.provider ?? "",
        model: config.embedding?.model ?? "",
        baseUrl: config.embedding?.base_url ?? "",
      });
      setLoaded(true);
      setError("");
    } catch (cause) {
      setLoaded(false);
      setError(`读取 Composer 推荐配置失败：${errText(cause)}`);
    }
  };
  onMount(() => void reload());

  const toggle = async (key: Flag) => {
    if (!loaded() || saving()) return;
    const next = !flags()[key];
    if (key === "semantic" && next && !embedding().provider) {
      setError("启用 Embedding 前必须先选择并保存 embedding provider");
      return;
    }
    if (
      next &&
      key !== "enabled" &&
      !window.confirm(
        key === "semantic"
          ? "启用后会把截断后的完整输入、近期 Session 文本和本地候选摘要发送给 embedding provider。确认启用？"
          : "启用后会把截断后的完整输入、近期 Session 文本、已选路径和本地候选 metadata 发送给 suggestion 角色模型。确认启用？",
      )
    )
      return;
    setSaving(key);
    setFlags((current) => ({ ...current, [key]: next }));
    try {
      await configSetComposerSuggestions(key, next);
      setError("");
    } catch (cause) {
      await reload();
      setError(`保存失败：${errText(cause)}`);
    } finally {
      setSaving("");
    }
  };

  const saveEmbedding = async () => {
    if (!loaded() || saving()) return;
    setSaving("embedding");
    try {
      const value = embedding();
      await configSetEmbedding(value.provider, value.model, value.baseUrl);
      setError("");
    } catch (cause) {
      await reload();
      setError(`保存 embedding 配置失败：${errText(cause)}`);
    } finally {
      setSaving("");
    }
  };

  const choices = [
    [
      "enabled",
      "上下文主动推荐",
      "默认开启，仅在本机按完整输入、Session 历史、附件、最近文件和 Git diff 排序，不访问网络",
    ],
    [
      "semantic",
      "Embedding semantic rerank",
      "默认关闭，仅对最多 8 个本地 shortlist 候选做语义重排；失败自动保留 Local 结果",
    ],
    [
      "llm",
      "LLM prompt suggest",
      "默认关闭，通过 suggestion 模型路由生成最多 3 个候选；文件 id 必须来自本地 shortlist",
    ],
  ] as const;

  return (
    <div class="space-y-3">
      <div>
        <div class="text-sm text-[var(--text)]">Composer 上下文主动推荐</div>
        <div class="text-xs text-[var(--text-faint)]">
          无需触发符。Trigger popup 优先；仅在光标位于文本末尾、非 IME、非录音和非运行状态显示。Tab
          接受，Enter 始终发送，Escape 对当前 draft 关闭。
        </div>
      </div>
      <Show when={error()}>
        <div class="text-xs text-[var(--err)]">{error()}</div>
      </Show>
      <For each={choices}>
        {([key, label, hint]) => (
          <div class="flex items-center justify-between gap-4 py-1">
            <div>
              <div class="text-xs text-[var(--text)]">{label}</div>
              <div class="text-2xs text-[var(--text-faint)]">{hint}</div>
            </div>
            <button
              class="pressable shrink-0 px-2.5 py-1 rounded-md text-xs border"
              disabled={!loaded() || Boolean(saving())}
              classList={{
                "border-[var(--warn)] text-[var(--warn)]": flags()[key],
                "border-[var(--border)] text-[var(--text-dim)]": !flags()[key],
              }}
              onClick={() => void toggle(key)}
            >
              {flags()[key] ? "已启用" : "已关闭"}
            </button>
          </div>
        )}
      </For>
      <div class="space-y-2 border-t border-[var(--border)] pt-3">
        <div class="text-xs text-[var(--text)]">Embedding provider</div>
        <div class="grid grid-cols-3 gap-2">
          <select
            class="form-select"
            value={embedding().provider}
            disabled={!loaded() || Boolean(saving())}
            onChange={(event) =>
              setEmbedding((current) => ({ ...current, provider: event.currentTarget.value }))
            }
          >
            <option value="">关闭</option>
            <option value="openai">OpenAI</option>
            <option value="openrouter">OpenRouter</option>
            <option value="ollama">Ollama</option>
          </select>
          <input
            class="form-input font-mono text-xs"
            value={embedding().model}
            placeholder="model，空值使用默认"
            disabled={!loaded() || Boolean(saving())}
            onInput={(event) =>
              setEmbedding((current) => ({ ...current, model: event.currentTarget.value }))
            }
          />
          <input
            class="form-input font-mono text-xs"
            value={embedding().baseUrl}
            placeholder="base URL，空值使用默认"
            disabled={!loaded() || Boolean(saving())}
            onInput={(event) =>
              setEmbedding((current) => ({ ...current, baseUrl: event.currentTarget.value }))
            }
          />
        </div>
        <div class="flex items-center justify-between gap-3 text-2xs text-[var(--text-faint)]">
          <span>API key 复用 Provider 凭证；远端只允许 HTTPS，HTTP 仅允许 loopback。</span>
          <button
            class="pressable shrink-0 rounded border border-[var(--border)] px-2.5 py-1 text-xs text-[var(--text)]"
            disabled={!loaded() || Boolean(saving())}
            onClick={() => void saveEmbedding()}
          >
            {saving() === "embedding" ? "保存中" : "保存 Embedding"}
          </button>
        </div>
      </div>
    </div>
  );
}
