import { createSignal, For, Show, onMount } from "solid-js";
import { A } from "@solidjs/router";
import { Activity, ArrowLeft } from "lucide-solid";
import { configGet, configSetRole, type RoleBindingView } from "../lib/chat";
import { theme, toggleTheme } from "../lib/theme";

const ROLE_LABELS: Record<string, string> = {
  chat: "主会话",
  thinking: "思考分析",
  planning: "任务规划",
  execution: "高速执行",
  review: "审查验证",
  research: "调研搜索",
};

const PROVIDERS = [
  { id: "anthropic", label: "Claude (订阅)" },
  { id: "openai", label: "GPT/Codex (订阅)" },
  { id: "xai", label: "Grok Build (订阅)" },
  { id: "kimi-for-coding", label: "Kimi Code (订阅)" },
];

const SECTIONS = ["通用", "模型路由", "用量与统计", "知识库 OKF", "高级"] as const;

export default function Settings() {
  const [section, setSection] = createSignal<(typeof SECTIONS)[number]>("通用");
  const [roles, setRoles] = createSignal<Record<string, RoleBindingView>>({});
  const [saved, setSaved] = createSignal("");

  onMount(async () => {
    const config = await configGet().catch(() => null);
    if (config?.roles) setRoles(config.roles);
  });

  const update = async (role: string, provider: string, model: string) => {
    await configSetRole(role, provider, model);
    setRoles((prev) => ({ ...prev, [role]: { provider, model } }));
    setSaved(`${role} 已保存并热生效`);
    setTimeout(() => setSaved(""), 2000);
  };

  return (
    <div class="h-full overflow-auto">
      <div class="h-8" data-tauri-drag-region />
      <div class="p-6 pt-2 max-w-4xl mx-auto flex gap-6">
        <nav class="w-36 shrink-0 space-y-0.5">
          <A
            href="/"
            class="flex items-center gap-1.5 text-xs text-[var(--text-dim)] hover:text-[var(--text)] mb-3"
          >
            <ArrowLeft size={13} />
            返回会话
          </A>
          {SECTIONS.map((s) => (
            <button
              class="w-full text-left px-2.5 py-1.5 rounded-md text-sm"
              classList={{
                "bg-[var(--bg-overlay)] text-[var(--text)]": section() === s,
                "text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60": section() !== s,
              }}
              onClick={() => setSection(s)}
            >
              {s}
            </button>
          ))}
        </nav>

        <div class="flex-1 min-w-0 space-y-4">
          <Show when={saved()}>
            <div class="text-xs text-[var(--ok)]">{saved()}</div>
          </Show>

          <Show when={section() === "通用"}>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 space-y-3">
              <h2 class="text-sm font-medium">外观</h2>
              <div class="flex items-center justify-between">
                <span class="text-sm text-[var(--text-dim)]">主题</span>
                <button
                  class="pressable px-3 py-1 rounded-md text-xs border border-[var(--border)]"
                  onClick={(e) => toggleTheme(e.clientX, e.clientY)}
                >
                  {theme() === "dark" ? "暗色" : "亮色"}（点击切换）
                </button>
              </div>
            </div>
          </Show>

          <Show when={section() === "模型路由"}>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
              <For each={Object.keys(ROLE_LABELS)}>
                {(role) => {
                  const binding = () => roles()[role] ?? { provider: "anthropic", model: "" };
                  return (
                    <div class="flex items-center gap-3 px-4 py-3">
                      <div class="w-20 shrink-0">
                        <div class="text-sm">{ROLE_LABELS[role]}</div>
                        <div class="text-[10px] text-[var(--text-faint)]">{role}</div>
                      </div>
                      <select
                        class="bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-xs text-[var(--text-dim)]"
                        value={binding().provider}
                        onChange={(e) =>
                          void update(
                            role,
                            e.currentTarget.value,
                            binding().model || defaultModelOf(e.currentTarget.value),
                          )
                        }
                      >
                        {PROVIDERS.map((p) => (
                          <option value={p.id}>{p.label}</option>
                        ))}
                      </select>
                      <input
                        class="flex-1 bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs font-mono"
                        value={binding().model}
                        placeholder="model id"
                        onChange={(e) =>
                          void update(role, binding().provider, e.currentTarget.value)
                        }
                      />
                    </div>
                  );
                }}
              </For>
            </div>
          </Show>

          <Show when={section() === "用量与统计"}>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 text-sm text-[var(--text-dim)]">
              会话 tokens 与状态栏用量段同步；明细（TTFT / tok/s）在每条 assistant 消息尾部。
            </div>
          </Show>

          <Show when={section() === "知识库 OKF"}>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 space-y-2 text-sm text-[var(--text-dim)]">
              <div>知识库目录：`.agents/`（项目，入 git）+ `.kxen/memory/`（本机）</div>
              <div class="text-xs text-[var(--text-faint)]">
                rules 全文注入；references 索引渐进披露；# 前缀快捷沉淀；/done 复盘沉淀（P1）。
              </div>
            </div>
          </Show>

          <Show when={section() === "高级"}>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 space-y-3">
              <div class="text-sm text-[var(--text-dim)]">
                hooks 在 `~/.config/kxen/config.toml` 的 [hooks] 配置（默认全关）。
              </div>
              <A
                href="/doctor"
                class="inline-flex items-center gap-1.5 text-xs text-[var(--accent-hover)]"
              >
                <Activity size={13} />
                打开环境检查
              </A>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}

function defaultModelOf(provider: string): string {
  return (
    {
      anthropic: "claude-sonnet-4-5-20250929",
      openai: "gpt-5.4",
      xai: "grok-build-0.1",
      "kimi-for-coding": "kimi-for-coding",
    }[provider] ?? ""
  );
}
