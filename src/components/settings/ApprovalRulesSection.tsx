// 审批规则区块（B1）：「本会话放行 / 总是放行」建规后的管理面——列表 + 撤销。
// session 规则随进程生命周期（内存），workspace 规则持久化在 <workspace>/.kxen/approval-rules.json。
import { createSignal, For, onMount, Show } from "solid-js";
import { Trash2 } from "lucide-solid";
import { flashErr, flashOk } from "../../lib/flash";
import { errText } from "../err-text";
import {
  approvalRulesList,
  approvalRulesRevoke,
  type ApprovalRule,
} from "../../lib/approval-rules";
import { activeSessionId } from "../../lib/state";
import { createSeqGuard } from "../../lib/async-guard";

const SCOPE_TEXT: Record<ApprovalRule["scope"], string> = {
  session: "本会话",
  workspace: "本 workspace",
};

function fmtTime(ms: number): string {
  const d = new Date(ms);
  const hm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  return `${d.getMonth() + 1}/${d.getDate()} ${hm}`;
}

export default function ApprovalRulesSection() {
  const [rules, setRules] = createSignal<ApprovalRule[]>([]);
  const [loaded, setLoaded] = createSignal(false);
  const [loadErr, setLoadErr] = createSignal("");
  // 撤销走行内确认条（对齐定时任务删除的二次确认模式）
  const [confirmRevoke, setConfirmRevoke] = createSignal("");
  const reloadGuard = createSeqGuard();
  const reload = async () => {
    const request = reloadGuard.next();
    try {
      // 带当前会话上下文：同时看到该会话规则与其 workspace 规则；无活跃会话时只看 workspace
      const list = await approvalRulesList(activeSessionId() || undefined);
      if (!reloadGuard.isCurrent(request)) return;
      setRules(list);
      setLoadErr("");
      setLoaded(true);
    } catch (error) {
      if (!reloadGuard.isCurrent(request)) return;
      const message = errText(error);
      setLoadErr(message);
      setLoaded(true);
      flashErr(`加载审批规则失败：${message}`);
    }
  };
  onMount(() => void reload());

  const revoke = async (rule: ApprovalRule) => {
    setConfirmRevoke("");
    const ok = await approvalRulesRevoke(rule.id, activeSessionId() || undefined).catch(
      (e: unknown) => {
        flashErr(`撤销失败：${errText(e)}`);
        return null;
      },
    );
    if (ok === null) return;
    flashOk(ok ? "审批规则已撤销" : "规则不存在（可能已随进程失效）");
    await reload();
  };

  return (
    <div class="space-y-3">
      <div class="text-xs text-[var(--text-faint)]">
        审批卡上「本会话放行 / 总是放行」建立的自动放行规则。命中规则的命令不再询问，
        但每次放行都会写入会话审计（decision=rule_allow）。含 shell
        元字符的复合命令永远不会被规则放行。
      </div>
      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
        <Show when={!loaded()}>
          <div class="px-4 py-3 text-xs text-[var(--text-faint)]">加载中…</div>
        </Show>
        <Show when={loaded() && loadErr()}>
          <div class="px-4 py-3 flex items-center gap-2 text-xs text-[var(--err)]">
            <span>
              {rules().length > 0 ? "刷新审批规则失败，正在显示上次结果" : "加载审批规则失败"}：
              {loadErr()}
            </span>
            <button
              class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-[var(--text-dim)]"
              onClick={() => void reload()}
            >
              重试
            </button>
          </div>
        </Show>
        <Show when={rules().length > 0}>
          <For each={rules()}>
            {(rule) => (
              <div class="px-4 py-3">
                <div class="flex items-center gap-2">
                  <span class="font-mono text-xs text-[var(--text)] break-all">{rule.prefix}</span>
                  <span class="text-2xs text-[var(--text-faint)] shrink-0">
                    {SCOPE_TEXT[rule.scope]}
                  </span>
                  <span class="ml-auto text-2xs text-[var(--text-faint)] shrink-0">
                    {fmtTime(rule.created_at_ms)} 创建
                  </span>
                  <button
                    class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-xs text-[var(--text)] flex items-center gap-1 shrink-0"
                    onClick={() => setConfirmRevoke(rule.id)}
                  >
                    <Trash2 size={10} />
                    撤销
                  </button>
                </div>
                <div class="mt-1 flex items-center gap-3 text-2xs text-[var(--text-faint)]">
                  <span>
                    已自动放行 {rule.used}
                    {rule.max_uses !== undefined ? ` / ${rule.max_uses}` : ""} 次
                  </span>
                  <Show when={rule.expires_at_ms !== undefined}>
                    <span>{fmtTime(rule.expires_at_ms!)} 过期</span>
                  </Show>
                  <Show when={rule.reason}>
                    <span class="truncate" title={rule.reason}>
                      来源：{rule.reason}
                    </span>
                  </Show>
                </div>
                <Show when={confirmRevoke() === rule.id}>
                  <div class="mt-2 rounded border border-[var(--warn)]/50 bg-[var(--warn)]/5 px-3 py-2 text-xs space-y-2">
                    <div class="text-[var(--warn)]">
                      {`确认撤销「${rule.prefix.slice(0, 40)}」的自动放行？撤销后该命令恢复逐次审批。`}
                    </div>
                    <div class="flex gap-2">
                      <button
                        class="pressable px-2 py-0.5 rounded text-2xs border border-[var(--err)] text-[var(--err)]"
                        onClick={() => void revoke(rule)}
                      >
                        确认撤销
                      </button>
                      <button
                        class="pressable px-2 py-0.5 rounded text-2xs border border-[var(--border)] text-[var(--text-dim)]"
                        onClick={() => setConfirmRevoke("")}
                      >
                        取消
                      </button>
                    </div>
                  </div>
                </Show>
              </div>
            )}
          </For>
        </Show>
        <Show when={loaded() && !loadErr() && rules().length === 0}>
          <div class="px-4 py-3 text-xs text-[var(--text-faint)]">
            暂无审批规则。在审批卡上点「本会话放行此命令」或「总是放行此命令」即可建立。
          </div>
        </Show>
      </div>
    </div>
  );
}
