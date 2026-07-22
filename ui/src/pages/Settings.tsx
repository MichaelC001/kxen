import { createSignal, Show } from "solid-js";
import { A } from "@solidjs/router";
import { ArrowLeft } from "lucide-solid";
import KnowledgeSection from "../components/settings/KnowledgeSection";
import ProvidersSection from "../components/settings/ProvidersSection";
import RoutingSection from "../components/settings/RoutingSection";
import VoiceSection from "../components/settings/VoiceSection";
import { theme, toggleTheme } from "../lib/theme";

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
            <ProvidersSection />
          </Show>

          <Show when={section() === "模型路由"}>
            <RoutingSection />
          </Show>

          <Show when={section() === "语音"}>
            <VoiceSection />
          </Show>

          <Show when={section() === "用量与统计"}>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 text-sm text-[var(--text-dim)]">
              会话 tokens 与状态栏用量段同步；每条 assistant 消息尾部有 TTFT / 耗时 / tok/s。
            </div>
          </Show>

          <Show when={section() === "知识库 OKF"}>
            <KnowledgeSection />
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
