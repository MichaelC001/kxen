// 消息操作条：复制全文 / 重新生成(assistant) / 编辑重发(user) / 分叉。hover 出现，图标化。
import { createSignal, Show } from "solid-js";
import { Check, Copy, GitFork, Pencil, RotateCcw } from "lucide-solid";

export default function MessageActions(props: {
  role: "user" | "assistant";
  content: string;
  onFork: () => void;
  onRerun?: () => void;
  onEditResend?: (text: string) => void;
}) {
  const [copied, setCopied] = createSignal(false);
  const [editing, setEditing] = createSignal(false);
  const [draft, setDraft] = createSignal("");
  let taRef: HTMLTextAreaElement | undefined;

  const copy = () => {
    void navigator.clipboard.writeText(props.content).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    });
  };

  const startEdit = () => {
    setDraft(props.content);
    setEditing(true);
    setTimeout(() => taRef?.focus(), 0);
  };

  const submit = () => {
    const t = draft().trim();
    if (t) props.onEditResend?.(t);
    setEditing(false);
  };

  const btn =
    "pressable px-1 py-0.5 rounded text-[var(--text-faint)] hover:text-[var(--text)] hover:bg-[var(--bg-overlay)]/70";

  return (
    <>
      <span class="inline-flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
        <button class={btn} title="复制全文" onClick={copy}>
          <Show when={copied()} fallback={<Copy size={11} />}>
            <Check size={11} class="text-[var(--ok)]" />
          </Show>
        </button>
        <Show when={props.role === "assistant" && props.onRerun}>
          <button class={btn} title="重新生成" onClick={() => props.onRerun?.()}>
            <RotateCcw size={11} />
          </button>
        </Show>
        <Show when={props.role === "user" && props.onEditResend}>
          <button class={btn} title="编辑重发（自动开分支）" onClick={startEdit}>
            <Pencil size={11} />
          </button>
        </Show>
        <button class={btn} title="从此消息分叉" onClick={props.onFork}>
          <GitFork size={11} />
        </button>
      </span>
      <Show when={editing()}>
        <div class="mt-1.5 rounded-lg border border-[var(--accent)] bg-[var(--bg-raised)] p-2 space-y-1.5">
          <textarea
            ref={(el) => (taRef = el)}
            class="w-full bg-transparent text-sm focus:outline-none resize-none"
            rows={3}
            value={draft()}
            onInput={(e) => setDraft(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submit();
              }
              if (e.key === "Escape") setEditing(false);
            }}
          />
          <div class="flex gap-1.5 justify-end">
            <button
              class="pressable px-2 py-0.5 rounded text-2xs border border-[var(--border)]"
              onClick={() => setEditing(false)}
            >
              取消
            </button>
            <button
              class="pressable px-2 py-0.5 rounded text-2xs bg-[var(--accent)] text-[var(--accent-contrast)]"
              onClick={submit}
            >
              重发（开分支）
            </button>
          </div>
        </div>
      </Show>
    </>
  );
}
