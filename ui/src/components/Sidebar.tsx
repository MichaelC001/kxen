import { createSignal, onMount } from "solid-js";
import { A } from "@solidjs/router";
import { currentModel } from "../lib/chat";

const NAV = [
  { path: "/", label: "会话", end: true },
  { path: "/goals", label: "目标" },
  { path: "/tasks", label: "任务" },
  { path: "/doctor", label: "环境" },
];

export default function Sidebar() {
  const [model, setModel] = createSignal("");
  onMount(async () => {
    const m = await currentModel();
    setModel(`${m.provider}/${m.model}`);
  });

  return (
    <nav class="w-48 shrink-0 flex flex-col border-r border-[var(--border)] bg-[var(--bg-raised)]">
      <div class="px-4 pt-4 pb-3 text-lg font-semibold tracking-tight text-[var(--accent-hover)]">
        kxen
      </div>
      <div class="px-2 space-y-0.5">
        {NAV.map((item) => (
          <A
            href={item.path}
            end={item.end}
            activeClass="bg-[var(--bg-overlay)] text-[var(--text)]"
            inactiveClass="text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
            class="block px-3 py-1.5 rounded-md text-sm"
          >
            {item.label}
          </A>
        ))}
      </div>
      <div class="mt-auto px-4 py-3 border-t border-[var(--border)]">
        <div class="text-[10px] uppercase tracking-wider text-[var(--text-faint)]">当前模型</div>
        <div class="text-xs text-[var(--text-dim)] truncate" title={model()}>
          {model() || "…"}
        </div>
      </div>
    </nav>
  );
}
