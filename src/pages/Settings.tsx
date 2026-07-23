import { createSignal, For, onMount, Show } from "solid-js";
import { A } from "@solidjs/router";
import { ArrowLeft } from "lucide-solid";
import KnowledgeSection from "../components/settings/KnowledgeSection";
import ProvidersSection from "../components/settings/ProvidersSection";
import RoutingSection from "../components/settings/RoutingSection";
import UsageSection from "../components/settings/UsageSection";
import VoiceSection from "../components/settings/VoiceSection";
import { client } from "../lib/client";
import { configGet } from "../lib/chat";
import { onDragStart } from "../lib/drag";
import { mode, setMode } from "../lib/theme";

const SECTIONS = [
  "通用",
  "提供商",
  "语音",
  "模型路由",
  "用量与统计",
  "知识库 OKF",
  "高级",
] as const;

export default function Settings() {
  const [section, setSection] = createSignal<(typeof SECTIONS)[number]>("通用");
  const [saved] = createSignal("");
  const [diagNote, setDiagNote] = createSignal("");
  const [sendPolicy, setSendPolicy] = createSignal("queue");

  onMount(async () => {
    const cfg = await configGet().catch(() => null);
    if (cfg?.send_when_running) setSendPolicy(cfg.send_when_running);
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

          <Show when={section() === "高级"}>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 space-y-3 text-sm text-[var(--text-dim)]">
              <div>
                hooks：`~/.config/kxen/config.toml` 的 [hooks]（默认全关，pre_tool_use 可阻断）
              </div>
              <div>statusline：同文件 [statusline] items 白名单控制显隐</div>
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
