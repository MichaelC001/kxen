// 审批审计区块（B7）：落盘 Part::Approval 的只读历史投影（时间倒序）。
// 数据源是全量会话 JSONL，撤销规则/删除会话都不会抹掉这里的决定痕迹。
import { createSignal, For, onMount, Show } from "solid-js";
import { flashErr } from "../../lib/flash";
import { errText } from "../err-text";
import { approvalHistory, type ApprovalHistoryEntry } from "../../lib/approval-rules";
import { createSeqGuard } from "../../lib/async-guard";

const DECISION_TEXT: Record<string, string> = {
  allow: "允许",
  deny: "拒绝",
  timeout: "超时未决",
  cancel: "已取消",
  rule_allow: "规则自动放行",
};

function fmtTime(ms: number): string {
  const d = new Date(ms);
  const hm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  return `${d.getMonth() + 1}/${d.getDate()} ${hm}`;
}

export default function ApprovalHistorySection() {
  const [rows, setRows] = createSignal<ApprovalHistoryEntry[]>([]);
  const [loaded, setLoaded] = createSignal(false);
  const [loadErr, setLoadErr] = createSignal("");
  const reloadGuard = createSeqGuard();
  const reload = async () => {
    const request = reloadGuard.next();
    try {
      const list = await approvalHistory(undefined, 100);
      if (!reloadGuard.isCurrent(request)) return;
      setRows(list);
      setLoadErr("");
      setLoaded(true);
    } catch (error) {
      if (!reloadGuard.isCurrent(request)) return;
      const message = errText(error);
      setLoadErr(message);
      setLoaded(true);
      flashErr(`加载审批历史失败：${message}`);
    }
  };
  onMount(() => void reload());

  return (
    <div class="space-y-3">
      <div class="text-xs text-[var(--text-faint)]">
        全部会话的审批决定（含规则自动放行），按时间倒序，只读。
      </div>
      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
        <Show when={!loaded()}>
          <div class="px-4 py-3 text-xs text-[var(--text-faint)]">加载中…</div>
        </Show>
        <Show when={loaded() && loadErr()}>
          <div class="px-4 py-3 flex items-center gap-2 text-xs text-[var(--err)]">
            <span>
              {rows().length > 0 ? "刷新审批历史失败，正在显示上次结果" : "加载审批历史失败"}：
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
        <Show when={rows().length > 0}>
          <For each={rows()}>
            {(row) => (
              <div class="px-4 py-2.5">
                <div class="flex items-center gap-2">
                  <span
                    class="text-2xs shrink-0"
                    classList={{
                      "text-[var(--ok)]": row.decision === "allow",
                      "text-[var(--err)]": row.decision === "deny",
                      "text-[var(--warn)]": row.decision === "rule_allow",
                      "text-[var(--text-faint)]": !["allow", "deny", "rule_allow"].includes(
                        row.decision,
                      ),
                    }}
                  >
                    {DECISION_TEXT[row.decision] ?? row.decision}
                  </span>
                  <span class="font-mono text-xs text-[var(--text)] break-all">{row.command}</span>
                  <span class="ml-auto text-2xs text-[var(--text-faint)] shrink-0">
                    {fmtTime(row.created_at)}
                  </span>
                </div>
                <div class="mt-0.5 flex items-center gap-3 text-2xs text-[var(--text-faint)]">
                  <span class="truncate" title={row.reason}>
                    {row.reason}
                  </span>
                  <span class="ml-auto shrink-0">会话 {row.session_id}</span>
                </div>
              </div>
            )}
          </For>
        </Show>
        <Show when={loaded() && !loadErr() && rows().length === 0}>
          <div class="px-4 py-3 text-xs text-[var(--text-faint)]">暂无审批记录</div>
        </Show>
      </div>
    </div>
  );
}
