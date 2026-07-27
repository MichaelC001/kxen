import { createSignal, onMount, Show } from "solid-js";
import { formatError } from "../../lib/error-text";
import {
  checkForUpdate,
  currentVersion,
  installUpdate,
  type AvailableUpdate,
} from "../../lib/updater";

export default function UpdateSection() {
  const [version, setVersion] = createSignal("");
  const [status, setStatus] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [update, setUpdate] = createSignal<AvailableUpdate | null>(null);

  onMount(() => {
    void currentVersion()
      .then(setVersion)
      .catch(() => setVersion("UNKNOWN"));
  });

  const check = async () => {
    setBusy(true);
    setStatus("正在检查更新");
    setUpdate(null);
    try {
      const available = await checkForUpdate();
      if (!available) {
        setStatus("当前已是最新版本");
        return;
      }
      setUpdate(available);
      setStatus(`发现版本 ${available.version}`);
    } catch (error) {
      setStatus(`检查失败：${formatError(error instanceof Error ? error.message : String(error))}`);
    } finally {
      setBusy(false);
    }
  };

  const install = async () => {
    const available = update();
    if (!available) return;
    setBusy(true);
    setStatus(`正在下载并安装 ${available.version}`);
    try {
      await installUpdate(available);
    } catch (error) {
      setStatus(`安装失败：${formatError(error instanceof Error ? error.message : String(error))}`);
      setBusy(false);
    }
  };

  return (
    <div class="flex items-center justify-between px-4 py-3">
      <div>
        <div class="text-sm">应用更新</div>
        <div class="text-xs text-[var(--text-faint)]">
          当前版本 {version() || "正在读取"}
          <Show when={status()}>，{status()}</Show>
        </div>
      </div>
      <div class="flex gap-1.5">
        <Show when={update()}>
          <button
            class="pressable px-2.5 py-1 rounded-md text-xs border border-[var(--accent)] text-[var(--accent-hover)]"
            disabled={busy()}
            onClick={() => void install()}
          >
            下载并安装
          </button>
        </Show>
        <button
          class="pressable px-2.5 py-1 rounded-md text-xs border border-[var(--border)] text-[var(--text)]"
          disabled={busy()}
          onClick={() => void check()}
        >
          {busy() ? "处理中" : "检查更新"}
        </button>
      </div>
    </div>
  );
}
