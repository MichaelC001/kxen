// 审批卡：Ask 档挂起命令的用户决定入口（允许/拒绝，决定后只读展示；超时/取消置灰色失效态）。
// 有会话归属且命令无 shell 元字符时提供「本会话放行」「总是放行」建规按钮（B1 审批规则）。
import { Show, createSignal } from "solid-js";
import type { ApprovalItem } from "../lib/items";
import Markdown from "./Markdown";

const RESOLVED_TEXT: Record<NonNullable<ApprovalItem["resolved"]>, string> = {
  allowed: "已允许",
  denied: "已拒绝",
  timeout: "已超时",
  cancelled: "已取消",
  expired: "已失效",
};

export type RememberScope = "session" | "workspace";

/** 与后端 approval_rules::has_metacharacters 同口径：元字符意味着前缀之外还藏着第二段动作，不可自动放行。 */
function hasShellMetacharacters(text: string): boolean {
  return /[;&|\n\r`$()<>\\]/.test(text);
}

export default function ApprovalCard(props: {
  item: ApprovalItem;
  onRespond: (id: string, allow: boolean, remember?: RememberScope) => Promise<void>;
}) {
  const [responding, setResponding] = createSignal(false);
  const respond = async (allow: boolean, remember?: RememberScope) => {
    if (responding()) return;
    setResponding(true);
    try {
      // 不带 remember 时不传第三参（既有调用方与测试断言保持两参形态）
      if (remember === undefined) {
        await props.onRespond(props.item.approvalId, allow);
      } else {
        await props.onRespond(props.item.approvalId, allow, remember);
      }
    } finally {
      setResponding(false);
    }
  };
  // 建规前提：归属会话已知（全局审批无会话规则落点）且命令本身可安全前缀匹配
  const canRemember = () => !!props.item.sessionId && !hasShellMetacharacters(props.item.command);
  // 非用户决定的 resolved（超时/取消/失效）：卡片转灰，与等待态的警示色拉开
  const invalid = () =>
    props.item.resolved === "timeout" ||
    props.item.resolved === "cancelled" ||
    props.item.resolved === "expired";
  // 动态工具生命周期（tool_define 注册 / tool_undefine 卸载）的 reason 是 markdown：
  // 描述 + 参数 Schema + 源码或 hash，走 Markdown 渲染拿高亮；其余审批 reason 保持纯文本（无 markdown 语义）
  const isToolDefine = () =>
    props.item.command.startsWith("tool_define ") ||
    props.item.command.startsWith("tool_undefine ");
  return (
    <div
      class="rounded-lg border px-3 py-2.5 text-xs space-y-2"
      classList={{
        "border-[var(--border)] bg-[var(--bg-raised)] opacity-70": invalid(),
        "border-[var(--warn)]/50 bg-[var(--warn)]/5": !invalid(),
      }}
    >
      <div class={invalid() ? "text-[var(--text-faint)]" : "text-[var(--warn)]"}>审批请求：</div>
      <Show
        when={isToolDefine()}
        fallback={
          <div class={invalid() ? "text-[var(--text-faint)]" : "text-[var(--warn)]"}>
            {props.item.reason}
          </div>
        }
      >
        <Markdown text={props.item.reason} />
      </Show>
      <div class="selectable font-mono text-[var(--text-dim)] break-all">{props.item.command}</div>
      <Show
        when={!props.item.resolved}
        fallback={
          <div class="text-2xs text-[var(--text-faint)]">
            {RESOLVED_TEXT[props.item.resolved ?? "expired"]}
          </div>
        }
      >
        <div class="flex flex-wrap gap-2">
          <button
            class="pressable px-2.5 py-1 rounded text-2xs bg-[var(--accent)] text-[var(--accent-contrast)]"
            disabled={responding()}
            onClick={() => void respond(true)}
          >
            允许
          </button>
          <button
            class="pressable px-2.5 py-1 rounded text-2xs border border-[var(--border)] text-[var(--err)]"
            disabled={responding()}
            onClick={() => void respond(false)}
          >
            拒绝
          </button>
          <Show when={canRemember()}>
            <button
              class="pressable px-2.5 py-1 rounded text-2xs border border-[var(--border)] text-[var(--text-dim)]"
              title="本次放行，且本会话内该命令前缀不再询问"
              disabled={responding()}
              onClick={() => void respond(true, "session")}
            >
              本会话放行此命令
            </button>
            <button
              class="pressable px-2.5 py-1 rounded text-2xs border border-[var(--border)] text-[var(--text-dim)]"
              title="本次放行，且该 workspace 内该命令前缀不再询问（可在设置页撤销）"
              disabled={responding()}
              onClick={() => void respond(true, "workspace")}
            >
              总是放行此命令
            </button>
          </Show>
        </div>
      </Show>
    </div>
  );
}
