// 存储消息 -> 时间线条目（工具调用/推理/文本/图片按序还原）。
import type { ContextItem, ModelIdentity, RunStats, StoredMessage } from "./chat";

export interface MsgItem {
  kind: "msg";
  role: "user" | "assistant";
  content: string;
  reasoning?: string | undefined;
  images?: { media_type: string; data: string }[] | undefined;
  stats?: RunStats | undefined;
  error?: string | undefined;
  /** Assistant 生成时的实际路由模型；旧消息缺省，不允许用当前 picker 值回填。 */
  model?: ModelIdentity | undefined;
  messageId?: string | undefined;
  /** 通知类 user 消息的来源小标（[teammate x] / [task notification] 前缀，与后端落盘文本同口径） */
  source?: string | undefined;
  /** 后端明确返回失败时的内存气泡；连接级 UNKNOWN 会撤下气泡并恢复到原会话 Composer。 */
  sendError?: string | undefined;
  /** unknown 表示连接在响应前中断，后端是否已接收不可判定，禁止一键盲重发。 */
  sendOutcome?: "failed" | "unknown" | undefined;
  /** 乐观气泡携带的 @ 引用原件：发送失败重发时原样带回，引用不丢 */
  context?: ContextItem[] | undefined;
  /** 旧 JSONL 只有展开快照，没有可逆 typed 引用；rerun/edit 必须阻断而非静默丢引用。 */
  contextUnavailable?: boolean | undefined;
}
export interface ToolItem {
  kind: "tool";
  name: string;
  call: string;
  args?: string | undefined;
  result?: string | undefined;
  /** 落盘来源定位（Chat「Inspect」联动 Trajectory 用）；流式态条目缺省。 */
  messageId?: string | undefined;
  partIndex?: number | undefined;
}
export interface PhaseItem {
  kind: "phase";
  name: string;
  /** 脚本声明 meta.phases 时带结构化进度（渲染进度条），否则只有文案 */
  index?: number | undefined;
  total?: number | undefined;
  workflow?: string | undefined;
}
/** auto-compact 现场卡（live-only，与 phase 同规：不落盘，刷新后消失）。 */
export interface CompactedItem {
  kind: "compacted";
  summary: string;
}
export interface ApprovalItem {
  kind: "approval";
  approvalId: string;
  command: string;
  reason: string;
  // 归属会话（实时事件/approval.pending 回填）；空 = 全局审批或落盘历史，不显示建规按钮
  sessionId?: string | undefined;
  // allowed/denied = 用户决定；timeout/cancelled = 后端了结（approval.resolved）；expired = 迟到应答发现服务端已了结
  resolved?: "allowed" | "denied" | "timeout" | "cancelled" | "expired";
}
export type Item = MsgItem | ToolItem | PhaseItem | CompactedItem | ApprovalItem;

/** 落盘 decision（allow/deny/timeout/cancel）-> 卡片已决态；未知值按 expired 兜底（不冒充用户决定）。 */
const DECISION_RESOLVED: Record<string, NonNullable<ApprovalItem["resolved"]>> = {
  allow: "allowed",
  deny: "denied",
  timeout: "timeout",
  cancel: "cancelled",
};

/** 通知类 user 消息的来源小标：[teammate 名] / [task notification] 前缀（后端落盘口径，见 drain_lead_inbox / drain_to_session）。 */
export function userSource(text: string): string | undefined {
  const teammate = /^\[teammate ([^\]]+)\]/.exec(text);
  if (teammate?.[1]) return `teammate ${teammate[1]}`;
  if (text.startsWith("[task notification]")) return "task notification";
  return undefined;
}

/** 剥离来源前缀后的正文（折叠卡标题摘要用）；无前缀原样返回。与 userSource 同口径。 */
export function userSourceBody(text: string): string {
  return text.replace(/^\[(?:teammate [^\]]+|task notification)\] ?/, "");
}

/** 单行摘要：首行截断 120 字符（记录表/折叠卡标题共用）。 */
export function firstLine(text: string): string {
  const line = text.split("\n", 1)[0] ?? "";
  return line.length > 120 ? `${line.slice(0, 120)}…` : line;
}

/** context 引用的一行来源描述（Chat 折叠卡标题 / Trajectory context 记录摘要共用）。 */
export function describeContextItem(item: ContextItem): string {
  switch (item.type) {
    case "file":
      return `file ${item.path}`;
    case "dir":
      return `dir ${item.path}`;
    case "web":
    case "docs":
      return `${item.type} ${item.url}`;
    case "note":
      return "note";
  }
}

export function describeContextItems(items: ContextItem[]): string {
  return items.map(describeContextItem).join("，");
}

function toolItem(
  p: StoredMessage["parts"][number],
  messageId?: string,
  partIndex?: number,
): ToolItem | undefined {
  if (p.type !== "tool_call" || !p.name) return undefined;
  return {
    kind: "tool",
    name: p.name,
    call: typeof p.input === "string" ? p.input : JSON.stringify(p.input),
    args: p.args == null ? undefined : JSON.stringify(p.args, null, 2),
    result: p.output || undefined,
    messageId,
    partIndex,
  };
}

/** 落盘的审批决定 -> 灰色已决历史卡（approvalId 空 = 无活体审批，按钮不出现）。 */
function approvalHistoryItem(p: StoredMessage["parts"][number]): ApprovalItem | undefined {
  if (p.type !== "approval" || p.command === undefined) return undefined;
  return {
    kind: "approval",
    approvalId: "",
    command: p.command,
    reason: p.reason ?? "",
    resolved: DECISION_RESOLVED[p.decision ?? ""] ?? "expired",
  };
}

/** 迭代消息 id = `{stream_id}:t{turn}`（crates/kxen-core/src/ws/llm_task/turn_persistence.rs）。
 *  `:` 不在后端 id 白名单内（core/ids.rs），存量 msg_* id 不会误匹配。 */
const ITERATION_ID = /^([A-Za-z0-9_-]+):t\d+$/;

/** 进行中的视觉回合：agent loop 每个迭代各落一条 Assistant 消息，同一 stream 的连续迭代消息
 *  + 紧随的一条收尾消息（Reasoning + 最终文本）属于同一回合——工具卡按序内嵌，全部文本
 *  进回合末尾单条气泡，与存量打包消息（Reasoning+ToolCall×N+Text 一条）渲染同形。 */
interface TurnAcc {
  stream: string;
  texts: string[];
  reasoning: string;
  images: { media_type: string; data: string }[];
  model?: ModelIdentity | undefined;
  lastMessageId: string;
}

/** 回合收尾合成末尾气泡；无可展示内容的纯工具回合不出气泡（崩溃无尾回合不留空白泡）。
 *  messageId 取回合内最后一条消息：fork 覆盖整个回合，rewind 自回合尾逐层回退。 */
function flushTurn(items: Item[], turn: TurnAcc | undefined): undefined {
  if (turn && (turn.texts.length > 0 || turn.reasoning || turn.images.length > 0)) {
    items.push({
      kind: "msg",
      role: "assistant",
      content: turn.texts.join("\n"),
      messageId: turn.lastMessageId,
      ...(turn.reasoning ? { reasoning: turn.reasoning } : {}),
      ...(turn.images.length > 0 ? { images: turn.images } : {}),
      ...(turn.model ? { model: turn.model } : {}),
    });
  }
  return undefined;
}

/** 回合内消息的 parts 归置：文本/reasoning/图片攒进回合气泡，tool/approval 按时序直接出条目。 */
function absorbTurnParts(items: Item[], turn: TurnAcc, m: StoredMessage): void {
  for (const [partIndex, p] of m.parts.entries()) {
    if (p.type === "text" && p.text) turn.texts.push(p.text);
    else if (p.type === "reasoning" && p.text) turn.reasoning += p.text;
    else if (p.type === "image" && p.media_type && p.data !== undefined)
      turn.images.push({ media_type: p.media_type, data: p.data });
    else {
      const item = toolItem(p, m.id, partIndex) ?? approvalHistoryItem(p);
      if (item) items.push(item);
    }
  }
  turn.lastMessageId = m.id;
  if (m.model) turn.model = m.model;
}

export function toItems(messages: StoredMessage[]): Item[] {
  const items: Item[] = [];
  let turn: TurnAcc | undefined;
  for (const m of messages) {
    if (m.role === "system") continue;
    if (m.role === "assistant") {
      const stream = ITERATION_ID.exec(m.id)?.[1];
      if (stream !== undefined) {
        if (turn?.stream !== stream) {
          turn = flushTurn(items, turn);
          turn = { stream, texts: [], reasoning: "", images: [], lastMessageId: m.id };
        }
        absorbTurnParts(items, turn, m);
        continue;
      }
      const hasToolCall = m.parts.some((p) => p.type === "tool_call");
      const inlineOnly = m.parts.every((p) => p.type === "approval" || p.type === "context");
      // 收尾消息口径（run_finalize/terminal.rs assemble_parts）：Reasoning + 最终文本，绝无 tool_call，
      // 直接并入开放回合。纯审批/纯 context（动态工具定义快照）/空 parts 消息是回合内联事件
      // （审批与快照落盘角色固定 Assistant）：出卡但不打断、不关闭回合。
      if (turn && !hasToolCall && !inlineOnly) {
        absorbTurnParts(items, turn, m);
        turn = flushTurn(items, turn);
        continue;
      }
      if (!inlineOnly) turn = flushTurn(items, turn);
      appendMessageItems(items, m);
      continue;
    }
    turn = flushTurn(items, turn);
    appendMessageItems(items, m);
  }
  flushTurn(items, turn);
  return items;
}

/** 单条消息逐 part 还原（存量打包 Assistant / user 消息，不做跨消息归并）。 */
function appendMessageItems(items: Item[], m: StoredMessage): void {
  if (m.role === "system") return;
  // reasoning 在 parts 里先于正文落盘（reasoning -> tool -> text）：先攒着，消息收尾时挂到本条 assistant 气泡
  let reasoning = "";
  for (const [partIndex, p] of m.parts.entries()) {
    if (p.type === "text" && p.text) {
      const last = items.at(-1);
      if (last?.kind === "msg" && last.role === m.role && last.messageId === m.id) {
        items[items.length - 1] = {
          ...last,
          content: `${last.content}\n${p.text}`,
          messageId: m.id,
        };
      } else {
        items.push({
          kind: "msg",
          role: m.role,
          content: p.text,
          messageId: m.id,
          source: m.role === "user" ? userSource(p.text) : undefined,
          ...(m.role === "assistant" && m.model ? { model: m.model } : {}),
        });
      }
    } else if (p.type === "reasoning" && p.text && m.role === "assistant") {
      reasoning += p.text;
    } else if (p.type === "context_sources" && p.items?.length && m.role === "user") {
      const last = items.at(-1);
      if (last?.kind === "msg" && last.role === "user" && last.messageId === m.id) {
        items[items.length - 1] = {
          ...last,
          context: [...(last.context ?? []), ...p.items],
          contextUnavailable: false,
        };
      } else {
        items.push({
          kind: "msg",
          role: "user",
          content: "",
          context: p.items,
          messageId: m.id,
        });
      }
    } else if (p.type === "context" && m.role === "user") {
      const last = items.at(-1);
      if (
        last?.kind === "msg" &&
        last.role === "user" &&
        last.messageId === m.id &&
        !last.context?.length
      ) {
        items[items.length - 1] = { ...last, contextUnavailable: true };
      }
    } else if (p.type === "image" && p.media_type && p.data !== undefined) {
      const img = { media_type: p.media_type, data: p.data };
      const last = items.at(-1);
      if (last?.kind === "msg" && last.role === m.role && last.messageId === m.id) {
        items[items.length - 1] = {
          ...last,
          images: [...(last.images ?? []), img],
          messageId: m.id,
        };
      } else {
        items.push({
          kind: "msg",
          role: m.role,
          content: "",
          images: [img],
          messageId: m.id,
          ...(m.role === "assistant" && m.model ? { model: m.model } : {}),
        });
      }
    } else {
      const item = toolItem(p, m.id, partIndex) ?? approvalHistoryItem(p);
      if (item) items.push(item);
    }
  }
  if (reasoning) {
    // 只往回扫本条消息的尾部条目（tool 条目无 messageId，扫到即说明本条没建气泡）
    let attached = false;
    for (let i = items.length - 1; i >= 0; i--) {
      const it = items[i];
      if (!it || it.kind !== "msg" || it.messageId !== m.id) break;
      if (it.role === "assistant") {
        items[i] = { ...it, reasoning: `${it.reasoning ?? ""}${reasoning}` };
        attached = true;
        break;
      }
    }
    // 纯思考无正文的极端情况也要补一条气泡，reasoning 不许静默丢
    if (!attached)
      items.push({
        kind: "msg",
        role: "assistant",
        content: "",
        reasoning,
        messageId: m.id,
        ...(m.model ? { model: m.model } : {}),
      });
  }
}
