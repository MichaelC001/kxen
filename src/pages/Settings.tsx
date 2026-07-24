import { createSignal, For, onMount, Show } from "solid-js";
import { A } from "@solidjs/router";
import { ArrowLeft } from "lucide-solid";
import KnowledgeSection from "../components/settings/KnowledgeSection";
import ProvidersSection from "../components/settings/ProvidersSection";
import RoutingSection from "../components/settings/RoutingSection";
import ScheduleSection from "../components/settings/ScheduleSection";
import UsageSection from "../components/settings/UsageSection";
import VoiceSection from "../components/settings/VoiceSection";
import { client } from "../lib/client";
import { configGet } from "../lib/chat";
import { mcpAuth, mcpRestart, mcpStatus, type McpServerStatus } from "../lib/mcp";
import { onDragStart } from "../lib/drag";
import { mode, setMode } from "../lib/theme";

const SECTIONS = [
  "通用",
  "提供商",
  "语音",
  "模型路由",
  "用量与统计",
  "知识库 OKF",
  "定时任务",
  "高级",
] as const;

export default function Settings() {
  const [section, setSection] = createSignal<(typeof SECTIONS)[number]>("通用");
  const [saved] = createSignal("");
  const [diagNote, setDiagNote] = createSignal("");
  const [sendPolicy, setSendPolicy] = createSignal("queue");
  const [mcpServers, setMcpServers] = createSignal<McpServerStatus[]>([]);
  // OAuth 授权中（等待浏览器回调）与待手动复制的授权 URL（后端没能拉起浏览器时）
  const [authPending, setAuthPending] = createSignal<Record<string, boolean>>({});
  const [authUrls, setAuthUrls] = createSignal<Record<string, string>>({});

  const refreshMcp = async () => {
    const list = await mcpStatus().catch(() => null);
    if (list) setMcpServers(list);
  };

  const startMcpAuth = async (name: string) => {
    setAuthPending((p) => ({ ...p, [name]: true }));
    const r = await mcpAuth(name).catch(() => null);
    if (!r) {
      setAuthPending((p) => ({ ...p, [name]: false }));
      return;
    }
    // 浏览器没拉起来：URL 展示出来供手动复制（授权流在后端照常等回调）
    if (!r.opened) setAuthUrls((p) => ({ ...p, [name]: r.authorize_url }));
    const clear = () => {
      setAuthPending((p) => ({ ...p, [name]: false }));
      setAuthUrls((p) => {
        const next = { ...p };
        delete next[name];
        return next;
      });
    };
    // 后端完成换 token 会自动重连：轮询直到脱离 needs_auth（上限与后端回调超时一致）
    const timer = setInterval(() => {
      void refreshMcp().then(() => {
        const cur = mcpServers().find((s) => s.name === name);
        if (cur && cur.status !== "needs_auth") {
          clearInterval(timer);
          clear();
        }
      });
    }, 2000);
    setTimeout(() => {
      clearInterval(timer);
      clear();
    }, 300_000);
  };

  onMount(async () => {
    const cfg = await configGet().catch(() => null);
    if (cfg?.send_when_running) setSendPolicy(cfg.send_when_running);
    void refreshMcp();
  });

  const setPolicy = async (p: string) => {
    setSendPolicy(p);
    await client.rpc("config.set_send_policy", { policy: p }).catch(() => {});
  };

  const exportDiag = async () => {
    const r = await client.rpc<{ path: string }>("diagnostics.export").catch(() => null);
    setDiagNote(r ? `已导出 ${r.path}` : "导出失败");
    setTimeout(() => setDiagNote(""), 3000);
  };

  return (
    <div class="h-full flex-1 overflow-auto">
      <div class="h-8" data-tauri-drag-region onMouseDown={onDragStart} />
      <div class="px-8 py-6 pt-2 flex gap-8">
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
                  <div class="text-xs text-[var(--text-faint)]">
                    跟随系统或手动固定，系统切换实时生效
                  </div>
                </div>
                <div class="flex gap-1">
                  <For each={["auto", "dark", "light"] as const}>
                    {(m) => (
                      <button
                        class="pressable px-2.5 py-1 rounded-md text-xs border"
                        classList={{
                          "border-[var(--accent)] text-[var(--accent-hover)]": mode() === m,
                          "border-[var(--border)] text-[var(--text-dim)]": mode() !== m,
                        }}
                        onClick={() => setMode(m)}
                      >
                        {m === "auto" ? "跟随系统" : m === "dark" ? "暗色" : "亮色"}
                      </button>
                    )}
                  </For>
                </div>
              </div>
              <div class="flex items-center justify-between px-4 py-3">
                <div>
                  <div class="text-sm">运行中发送</div>
                  <div class="text-xs text-[var(--text-faint)]">
                    生成中再发消息：排队等当前完成，或打断当前立即发送
                  </div>
                </div>
                <div class="flex gap-1">
                  <For each={["queue", "interrupt"] as const}>
                    {(p) => (
                      <button
                        class="pressable px-2.5 py-1 rounded-md text-xs border"
                        classList={{
                          "border-[var(--accent)] text-[var(--accent-hover)]": sendPolicy() === p,
                          "border-[var(--border)] text-[var(--text-dim)]": sendPolicy() !== p,
                        }}
                        onClick={() => void setPolicy(p)}
                      >
                        {p === "queue" ? "排队" : "打断"}
                      </button>
                    )}
                  </For>
                </div>
              </div>
            </div>
          </Show>

          <Show when={section() === "提供商"}>
            <ProvidersSection />
          </Show>

          <Show when={section() === "模型路由"}>
            <RoutingSection />
          </Show>

          <Show when={section() === "语音"}>
            <VoiceSection />
          </Show>

          <Show when={section() === "用量与统计"}>
            <UsageSection />
          </Show>

          <Show when={section() === "知识库 OKF"}>
            <KnowledgeSection />
          </Show>

          <Show when={section() === "定时任务"}>
            <ScheduleSection />
          </Show>

          <Show when={section() === "高级"}>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 space-y-3 text-sm text-[var(--text-dim)]">
              <div>
                hooks：`~/.config/kxen/config.toml` 的 [hooks]（默认全关，pre_tool_use 可阻断）
              </div>
              <div>statusline：同文件 [statusline] items 白名单控制显隐</div>
              <div class="pt-2 border-t border-[var(--border)]">
                <div class="mb-1.5 text-xs text-[var(--text)]">
                  MCP servers（.mcp.json / ~/.config/kxen/mcp.json）
                </div>
                <Show
                  when={mcpServers().length > 0}
                  fallback={<div class="text-xs">未配置 MCP server</div>}
                >
                  <For each={mcpServers()}>
                    {(s) => (
                      <div class="py-1 text-xs">
                        <div class="flex items-center gap-2">
                          <span
                            class="inline-block w-2 h-2 rounded-full"
                            style={{
                              "background-color":
                                s.status === "running"
                                  ? "var(--ok)"
                                  : s.status === "needs_auth"
                                    ? "var(--warn)"
                                    : "var(--err)",
                            }}
                          />
                          <span class="text-[var(--text)]">{s.name}</span>
                          <span class="text-[var(--text-dim)]">{s.transport}</span>
                          <Show when={s.url}>
                            {(u) => <span class="truncate text-[var(--text-dim)]">{u()}</span>}
                          </Show>
                          <span class="text-[var(--text-dim)]">{s.tools} tools</span>
                          <Show when={s.resources > 0}>
                            <span class="text-[var(--text-dim)]">{s.resources} resources</span>
                          </Show>
                          <Show when={s.status === "needs_auth"}>
                            <button
                              class="pressable ml-auto px-2 py-0.5 rounded border border-[var(--warn)] text-[var(--warn)] disabled:opacity-50"
                              disabled={!!authPending()[s.name]}
                              onClick={() => void startMcpAuth(s.name)}
                            >
                              {authPending()[s.name] ? "等待授权…" : "认证"}
                            </button>
                          </Show>
                          <button
                            class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-[var(--text)]"
                            classList={{ "ml-auto": s.status !== "needs_auth" }}
                            onClick={() => void mcpRestart(s.name).then(refreshMcp)}
                          >
                            重启
                          </button>
                        </div>
                        <Show when={authUrls()[s.name]}>
                          {(u) => (
                            <div class="mt-1 flex items-center gap-2 pl-4">
                              <span class="text-[var(--text-dim)]">浏览器未打开，请手动访问：</span>
                              <code class="flex-1 truncate text-[var(--text)] select-all">{u()}</code>
                              <button
                                class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-[var(--text)]"
                                onClick={() => void navigator.clipboard.writeText(u())}
                              >
                                复制
                              </button>
                            </div>
                          )}
                        </Show>
                      </div>
                    )}
                  </For>
                </Show>
              </div>
              <div class="pt-1 border-t border-[var(--border)] flex items-center gap-3">
                <button
                  class="pressable px-3 py-1.5 rounded-md text-xs border border-[var(--border)] text-[var(--text)]"
                  onClick={() => void exportDiag()}
                >
                  导出诊断包（markdown）
                </button>
                <Show when={diagNote()}>
                  <span class="text-xs text-[var(--ok)]">{diagNote()}</span>
                </Show>
              </div>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}
