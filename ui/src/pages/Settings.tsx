import { createSignal, For, Show, onMount } from "solid-js";
import { A } from "@solidjs/router";
import { ArrowLeft, RefreshCw } from "lucide-solid";
import {
  configGet,
  configSetRole,
  doctor,
  type DoctorReport,
  type RoleBindingView,
} from "../lib/chat";
import {
  setVoiceEngine,
  setVoiceProviderKey,
  voiceEngines,
  type VoiceOverview,
} from "../lib/voice";
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
  { id: "anthropic", label: "Claude" },
  { id: "openai", label: "GPT/Codex" },
  { id: "xai", label: "Grok Build" },
  { id: "kimi-for-coding", label: "Kimi Code" },
];

const SECTIONS = [
  "通用",
  "提供商",
  "语音",
  "模型路由",
  "用量与统计",
  "知识库 OKF",
  "高级",
] as const;

const STATUS_STYLE: Record<string, { text: string; cls: string }> = {
  imported: { text: "已导入", cls: "text-[var(--ok)]" },
  ok: { text: "正常", cls: "text-[var(--ok)]" },
  missing: { text: "缺失", cls: "text-[var(--warn)]" },
  expired: { text: "过期", cls: "text-[var(--err)]" },
  ready: { text: "就绪", cls: "text-[var(--ok)]" },
  needs_auth: { text: "待授权", cls: "text-[var(--warn)]" },
  unconfigured: { text: "未配置", cls: "text-[var(--warn)]" },
  unavailable: { text: "不可用", cls: "text-[var(--err)]" },
};

export default function Settings() {
  const [section, setSection] = createSignal<(typeof SECTIONS)[number]>("通用");
  const [roles, setRoles] = createSignal<Record<string, RoleBindingView>>({});
  const [report, setReport] = createSignal<DoctorReport | null>(null);
  const [doctorLoading, setDoctorLoading] = createSignal(false);
  const [saved, setSaved] = createSignal("");
  const [voiceOv, setVoiceOv] = createSignal<VoiceOverview | null>(null);
  const [voiceKeys, setVoiceKeys] = createSignal<Record<string, string>>({});

  const reloadVoice = async () => setVoiceOv(await voiceEngines().catch(() => null));

  const runDoctor = async () => {
    setDoctorLoading(true);
    try {
      setReport(await doctor());
    } finally {
      setDoctorLoading(false);
    }
  };

  onMount(async () => {
    const config = await configGet().catch(() => null);
    if (config?.roles) setRoles(config.roles);
    void runDoctor();
    void reloadVoice();
  });

  const saveVoiceKey = async (provider: string) => {
    const key = (voiceKeys()[provider] ?? "").trim();
    if (!key) return;
    await setVoiceProviderKey(provider, key);
    setVoiceKeys((prev) => ({ ...prev, [provider]: "" }));
    await reloadVoice();
    setSaved(`${provider} 转写 key 已保存`);
    setTimeout(() => setSaved(""), 2000);
  };

  const switchVoiceEngine = async (engine: string) => {
    await setVoiceEngine(engine, voiceOv()?.fallback ?? []);
    await reloadVoice();
    setSaved("语音引擎已切换并热生效");
    setTimeout(() => setSaved(""), 2000);
  };

  const update = async (role: string, provider: string, model: string) => {
    await configSetRole(role, provider, model);
    setRoles((prev) => ({ ...prev, [role]: { provider, model } }));
    setSaved(`${ROLE_LABELS[role] ?? role} 已保存并热生效`);
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
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
              <div class="flex items-center justify-between px-4 py-3">
                <div>
                  <div class="text-sm">主题</div>
                  <div class="text-xs text-[var(--text-faint)]">明暗切换，跟随系统默认</div>
                </div>
                <button
                  class="pressable px-3 py-1 rounded-md text-xs border border-[var(--border)]"
                  onClick={(e) => toggleTheme(e.clientX, e.clientY)}
                >
                  {theme() === "dark" ? "暗色" : "亮色"}
                </button>
              </div>
            </div>
          </Show>

          <Show when={section() === "提供商"}>
            <div class="flex items-center justify-between">
              <div class="text-xs text-[var(--text-faint)]">
                订阅凭证状态（官方 CLI 凭证复用，零明文存储）
              </div>
              <button
                class="pressable flex items-center gap-1 px-2 py-1 rounded text-xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
                onClick={() => void runDoctor()}
                disabled={doctorLoading()}
              >
                <RefreshCw size={12} class={doctorLoading() ? "animate-spin" : ""} />
                重新检查
              </button>
            </div>
            <Show when={report()}>
              {(r) => (
                <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
                  <For each={r().entries}>
                    {(entry) => {
                      const badge = () =>
                        STATUS_STYLE[entry.status] ?? { text: entry.status, cls: "" };
                      return (
                        <div class="flex items-center justify-between px-4 py-3">
                          <div>
                            <div class="text-sm font-medium">{entry.display}</div>
                            <div class="text-xs text-[var(--text-faint)]">{entry.provider}</div>
                          </div>
                          <div class="text-right">
                            <div class={`text-sm font-medium ${badge().cls}`}>{badge().text}</div>
                            <div class="text-xs text-[var(--text-faint)]">{entry.detail}</div>
                          </div>
                        </div>
                      );
                    }}
                  </For>
                </div>
              )}
            </Show>
            <Show when={report()}>
              {(r) => (
                <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-3 text-xs font-mono text-[var(--text-dim)] space-y-1">
                  <div>{r().bun_like_runtime}</div>
                  <div>data: {r().data_dir}</div>
                  <div>config: {r().config_dir}</div>
                </div>
              )}
            </Show>
          </Show>

          <Show when={section() === "模型路由"}>
            <div class="text-xs text-[var(--text-faint)]">
              不同用途走不同订阅/模型（MRM 全局调度，改动热生效）
            </div>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
              <For each={Object.keys(ROLE_LABELS)}>
                {(role) => {
                  const binding = () => roles()[role] ?? { provider: "anthropic", model: "" };
                  return (
                    <div class="flex items-center gap-3 px-4 py-3">
                      <div class="w-20 shrink-0">
                        <div class="text-sm">{ROLE_LABELS[role]}</div>
                        <div class="text-2xs text-[var(--text-faint)]">{role}</div>
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

          <Show when={section() === "语音"}>
            <div class="text-xs text-[var(--text-faint)]">
              主引擎 Apple 本地识别（离线零成本）；provider 转写为可切换引擎与降级链
            </div>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
              <For each={voiceOv()?.engines ?? []}>
                {(e) => {
                  const badge = () => STATUS_STYLE[e.status] ?? { text: e.status, cls: "" };
                  return (
                    <div class="flex items-center justify-between px-4 py-3">
                      <div>
                        <div class="text-sm font-medium">{e.label}</div>
                        <div class="text-xs text-[var(--text-faint)]">{e.id}</div>
                      </div>
                      <div class="flex items-center gap-3">
                        <div class="text-right">
                          <div class={`text-sm font-medium ${badge().cls}`}>{badge().text}</div>
                          <div class="text-xs text-[var(--text-faint)]">{e.detail}</div>
                        </div>
                        <button
                          class="pressable px-2.5 py-1 rounded text-xs border border-[var(--border)]"
                          classList={{ "opacity-40": e.status === "unavailable" }}
                          disabled={voiceOv()?.engine === e.id || e.status === "unavailable"}
                          onClick={() => void switchVoiceEngine(e.id)}
                        >
                          {voiceOv()?.engine === e.id ? "当前引擎" : "设为主引擎"}
                        </button>
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
              <For each={["openai", "xai"]}>
                {(p) => (
                  <div class="flex items-center gap-3 px-4 py-3">
                    <div class="w-24 shrink-0 text-sm">{p} 转写 key</div>
                    <input
                      type="password"
                      class="flex-1 bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs font-mono"
                      placeholder="sk-...（仅存本机 auth.json，0600）"
                      value={voiceKeys()[p] ?? ""}
                      onInput={(e) =>
                        setVoiceKeys((prev) => ({ ...prev, [p]: e.currentTarget.value }))
                      }
                    />
                    <button
                      class="pressable px-2.5 py-1 rounded text-xs border border-[var(--border)]"
                      onClick={() => void saveVoiceKey(p)}
                    >
                      保存
                    </button>
                  </div>
                )}
              </For>
            </div>
          </Show>

          <Show when={section() === "用量与统计"}>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 text-sm text-[var(--text-dim)]">
              会话 tokens 与状态栏用量段同步；每条 assistant 消息尾部有 TTFT / 耗时 / tok/s。
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
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 space-y-2 text-sm text-[var(--text-dim)]">
              <div>
                hooks：`~/.config/kxen/config.toml` 的 [hooks]（默认全关，pre_tool_use 可阻断）
              </div>
              <div>statusline：同文件 [statusline] items 白名单控制显隐</div>
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
