// 语音区：引擎状态/切换 + provider key 配置。
import { createSignal, For, onMount, Show } from "solid-js";
import {
  setVoiceEngine,
  setVoiceProviderKey,
  voiceEngines,
  type VoiceOverview,
} from "../../lib/voice";

const BADGE: Record<string, { text: string; cls: string }> = {
  ready: { text: "就绪", cls: "text-[var(--ok)]" },
  needs_auth: { text: "待授权", cls: "text-[var(--warn)]" },
  unconfigured: { text: "未配置", cls: "text-[var(--warn)]" },
  unavailable: { text: "不可用", cls: "text-[var(--err)]" },
};

export default function VoiceSection() {
  const [ov, setOv] = createSignal<VoiceOverview | null>(null);
  const [keys, setKeys] = createSignal<Record<string, string>>({});
  const [saved, setSaved] = createSignal("");

  const reload = async () => setOv(await voiceEngines().catch(() => null));
  onMount(() => void reload());

  const flash = (msg: string) => {
    setSaved(msg);
    setTimeout(() => setSaved(""), 2000);
  };

  const switchEngine = async (engine: string) => {
    await setVoiceEngine(engine, ov()?.fallback ?? []);
    await reload();
    flash("语音引擎已切换并热生效");
  };

  const saveKey = async (provider: string) => {
    const key = (keys()[provider] ?? "").trim();
    if (!key) return;
    await setVoiceProviderKey(provider, key);
    setKeys((prev) => ({ ...prev, [provider]: "" }));
    await reload();
    flash(`${provider} 转写 key 已保存`);
  };

  return (
    <>
      <div class="text-xs text-[var(--text-faint)]">
        主引擎 Apple 本地识别（离线零成本）；provider 转写为可切换引擎与降级链
      </div>
      <Show when={saved()}>
        <div class="text-xs text-[var(--ok)]">{saved()}</div>
      </Show>
      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
        <For each={ov()?.engines ?? []}>
          {(e) => {
            const badge = () => BADGE[e.status] ?? { text: e.status, cls: "" };
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
                    disabled={ov()?.engine === e.id || e.status === "unavailable"}
                    onClick={() => void switchEngine(e.id)}
                  >
                    {ov()?.engine === e.id ? "当前引擎" : "设为主引擎"}
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
                value={keys()[p] ?? ""}
                onInput={(e) => setKeys((prev) => ({ ...prev, [p]: e.currentTarget.value }))}
              />
              <button
                class="pressable px-2.5 py-1 rounded text-xs border border-[var(--border)]"
                onClick={() => void saveKey(p)}
              >
                保存
              </button>
            </div>
          )}
        </For>
      </div>
    </>
  );
}
