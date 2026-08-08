// 看板自主授权编辑（kanban.policy_set 的人类入口）：allowlist 每行一条命令前缀，
// max_uses/时限留空 = 不限；时限分钟数在提交时换算 expires_at_ms = now + 分钟。
// 重设即重置计数（核心 PolicySet 语义）：表单永远承载完整新授权，不做增量编辑。
import { createSignal, Show } from "solid-js";
import { ShieldCheck } from "lucide-solid";
import type { KanbanPolicySpec } from "../lib/chat";
import { relTime } from "../lib/time";

export default function KanbanPolicy(props: {
  policy: { spec: KanbanPolicySpec; used: number } | null | undefined;
  acting: boolean;
  onSave: (policy: KanbanPolicySpec) => void;
  onClose: () => void;
}) {
  const current = () => props.policy ?? null;
  const [allowlist, setAllowlist] = createSignal(current()?.spec.allowlist.join("\n") ?? "");
  const [maxUses, setMaxUses] = createSignal(current()?.spec.max_uses?.toString() ?? "");
  const [minutes, setMinutes] = createSignal("");

  const prefixes = () =>
    allowlist()
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.length > 0);

  const expired = () => {
    const at = current()?.spec.expires_at_ms;
    return at != null && at <= Date.now();
  };

  const save = () => {
    const uses = maxUses().trim() ? Number(maxUses().trim()) : undefined;
    const mins = minutes().trim() ? Number(minutes().trim()) : undefined;
    const policy: KanbanPolicySpec = { allowlist: prefixes() };
    if (uses && uses > 0) policy.max_uses = Math.floor(uses);
    if (mins && mins > 0) policy.expires_at_ms = Date.now() + Math.floor(mins) * 60_000;
    props.onSave(policy);
  };

  return (
    <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-3 space-y-2">
      <div class="flex items-center gap-2 text-xs text-[var(--text)]">
        <ShieldCheck size={13} class="text-[var(--text-dim)] shrink-0" />
        自主授权
        <Show
          when={current()}
          fallback={
            <span class="text-2xs text-[var(--text-faint)]">
              当前未授权：列 Agent 的高危命令仍逐条审批
            </span>
          }
        >
          {(p) => (
            <span class="text-2xs text-[var(--text-faint)]">
              当前：{p().spec.allowlist.length} 条前缀，已放行 {p().used}
              {p().spec.max_uses != null ? `/${p().spec.max_uses}` : "（不限次）"}
              {p().spec.expires_at_ms != null &&
                (expired() ? "，已过期" : `，${relTime(p().spec.expires_at_ms!)} 过期`)}
            </span>
          )}
        </Show>
      </div>
      <textarea
        class="w-full px-2 py-1.5 rounded border border-[var(--border)] bg-transparent text-xs font-mono resize-none"
        rows={4}
        placeholder={"允许自动放行的命令前缀，每行一条，例如：\ncargo test\ncargo clippy"}
        aria-label="授权命令前缀"
        value={allowlist()}
        onInput={(e) => setAllowlist(e.currentTarget.value)}
      />
      <div class="flex items-center gap-2">
        <input
          class="w-28 px-2 py-1.5 rounded border border-[var(--border)] bg-transparent text-xs"
          type="number"
          min="1"
          placeholder="次数（空=不限）"
          aria-label="最大自动放行次数"
          value={maxUses()}
          onInput={(e) => setMaxUses(e.currentTarget.value)}
        />
        <input
          class="w-32 px-2 py-1.5 rounded border border-[var(--border)] bg-transparent text-xs"
          type="number"
          min="1"
          placeholder="时限分钟（空=不限）"
          aria-label="授权时限分钟数"
          value={minutes()}
          onInput={(e) => setMinutes(e.currentTarget.value)}
        />
        <button
          class="pressable px-3 py-1.5 rounded border border-[var(--border)] text-xs disabled:opacity-40"
          disabled={props.acting || prefixes().length === 0}
          onClick={save}
        >
          保存授权
        </button>
        <button
          class="pressable px-3 py-1.5 rounded border border-[var(--border)] text-xs text-[var(--text-dim)]"
          onClick={() => props.onClose()}
        >
          收起
        </button>
      </div>
      <p class="text-2xs text-[var(--text-faint)]">
        保存会替换现有授权并重置已放行计数；Deny 档命令永不自动放行
      </p>
    </div>
  );
}
