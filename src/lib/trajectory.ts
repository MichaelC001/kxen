// Trajectory 投影：append-only 会话事件流的第二个 read model（Chat 负责读，Trajectory 负责查）。
// 记录类型封闭集合：system / user / context / compacted / message / tool / subtool。
// 核心原则：进行中/未知数据一律缺省（undefined），绝不编造数字；来源归因从持久字段读出。
import type { ContextItem, MessageRunStats, ModelIdentity, StoredMessage } from "./chat";
import { describeContextItems, firstLine, userSource } from "./items";

/** 与后端 session_compaction COMPACT_MARK 同口径：摘要消息首行标记。 */
export const COMPACT_MARK = "[earlier summary]";

/** 时长格式化：未知由调用方处理（不传进来）；<1s 给毫秒，<1min 给秒，否则分秒。 */
export function formatMs(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const minutes = Math.floor(ms / 60_000);
  return `${minutes}m ${Math.round((ms % 60_000) / 1000)}s`;
}

/** 时刻格式化（HH:MM:SS.mmm 本地时区）。 */
export function formatClock(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
}

export type TrajectoryKind =
  | "system"
  | "user"
  | "context"
  | "compacted"
  | "message"
  | "tool"
  | "subtool";

export interface TrajectoryTool {
  name: string;
  /** 一行摘要（落盘 input 字段） */
  call: string;
  /** 精确 arguments（JSON 格式化；存量缺省 = undefined） */
  args?: string | undefined;
  result?: string | undefined;
  callId?: string | undefined;
  startedAt?: number | undefined;
  finishedAt?: number | undefined;
}

export interface TrajectoryRecord {
  /** 全表序号（#N），投影时按事件序分配，过滤/分页不改变它 */
  index: number;
  kind: TrajectoryKind;
  /** 所属落盘消息 id（Inspect 联动定位用） */
  messageId: string;
  /** 消息内 part 下标（Inspect 联动定位用） */
  partIndex: number;
  /** 落盘 created_at（ms epoch）；tool 有实测起止时它是落盘时刻不是执行时刻 */
  time: number;
  role?: "user" | "assistant" | "system" | undefined;
  /** 记录表内容列的一行摘要 */
  summary: string;
  /** 完整文本（Inspector 渲染；message/user/system/context/compacted） */
  text?: string | undefined;
  reasoning?: string | undefined;
  images?: { media_type: string; data: string }[] | undefined;
  /** Assistant 生成时的实际路由模型（旧消息缺省） */
  model?: ModelIdentity | null | undefined;
  /** run 收尾消息的统计快照（TTFT/耗时/token；缺省 = unknown） */
  stats?: MessageRunStats | undefined;
  /** 来源归因：teammate 名 / task notification / 上下文来源项，全部从持久字段读出 */
  source?: string | undefined;
  /** context 记录的可逆 typed 引用（context_sources 落盘件） */
  contextItems?: ContextItem[] | undefined;
  tool?: TrajectoryTool | undefined;
}

/** 工具耗时（ms）：起止齐全才返回；缺一 = unknown（undefined）。 */
export function toolDurationMs(tool: TrajectoryTool): number | undefined {
  if (tool.startedAt === undefined || tool.finishedAt === undefined) return undefined;
  return Math.max(0, tool.finishedAt - tool.startedAt);
}

/** 存储消息 -> Trajectory 记录。tool_call 的 call+result 落盘时已在同一 part（output 已填），
 *  这里一行出；approval 落盘块按 tool 记录出（name=approval），它是动作决策事件。 */
export function toTrajectoryRecords(messages: StoredMessage[]): TrajectoryRecord[] {
  const records: TrajectoryRecord[] = [];
  const push = (
    m: StoredMessage,
    partIndex: number,
    partial: Omit<TrajectoryRecord, "index" | "messageId" | "partIndex" | "time">,
  ) => {
    records.push({
      index: records.length,
      messageId: m.id,
      partIndex,
      time: m.created_at,
      ...partial,
    });
  };
  for (const m of messages) {
    if (m.role === "system") {
      m.parts.forEach((p, partIndex) => {
        if (p.type === "text" && p.text)
          push(m, partIndex, {
            kind: "system",
            role: "system",
            summary: firstLine(p.text),
            text: p.text,
          });
      });
      continue;
    }
    let reasoning = "";
    m.parts.forEach((p, partIndex) => {
      if (p.type === "reasoning" && p.text) {
        reasoning += p.text;
        return;
      }
      if (p.type === "text" && p.text) {
        if (m.role === "user" && p.text.startsWith(COMPACT_MARK)) {
          const summary = p.text.slice(COMPACT_MARK.length).replace(/^\n/, "");
          push(m, partIndex, {
            kind: "compacted",
            role: "user",
            summary: firstLine(summary) || "（空摘要）",
            text: summary,
          });
          return;
        }
        const kind = m.role === "user" ? "user" : "message";
        push(m, partIndex, {
          kind,
          role: m.role,
          summary: firstLine(p.text),
          text: p.text,
          ...(reasoning ? { reasoning } : {}),
          ...(m.role === "assistant" && m.model ? { model: m.model } : {}),
          ...(m.role === "assistant" && m.stats ? { stats: m.stats } : {}),
          ...(m.role === "user" ? { source: userSource(p.text) } : {}),
        });
        reasoning = "";
        return;
      }
      if (p.type === "context" && p.text && m.role === "user") {
        push(m, partIndex, {
          kind: "context",
          role: "user",
          summary: firstLine(p.text),
          text: p.text,
        });
        return;
      }
      if (p.type === "context_sources" && p.items?.length && m.role === "user") {
        push(m, partIndex, {
          kind: "context",
          role: "user",
          summary: describeContextItems(p.items),
          source: describeContextItems(p.items),
          contextItems: p.items,
        });
        return;
      }
      if (p.type === "image" && p.media_type && p.data !== undefined) {
        push(m, partIndex, {
          kind: m.role === "user" ? "user" : "message",
          role: m.role,
          summary: `[图片 ${p.media_type}]`,
          images: [{ media_type: p.media_type, data: p.data }],
          ...(m.role === "assistant" && m.model ? { model: m.model } : {}),
        });
        return;
      }
      if (p.type === "tool_call" && p.name) {
        push(m, partIndex, {
          kind: "tool",
          role: m.role,
          summary: `${p.name} ${typeof p.input === "string" ? p.input : JSON.stringify(p.input)}`,
          tool: {
            name: p.name,
            call: typeof p.input === "string" ? p.input : JSON.stringify(p.input),
            args: p.args == null ? undefined : JSON.stringify(p.args, null, 2),
            result: p.output || undefined,
            callId: p.id ?? undefined,
            startedAt: p.started_at,
            finishedAt: p.finished_at,
          },
        });
        return;
      }
      if (p.type === "approval" && p.command !== undefined) {
        push(m, partIndex, {
          kind: "tool",
          role: m.role,
          summary: `approval ${p.command}`,
          tool: { name: "approval", call: p.command, result: p.decision },
        });
      }
    });
    // 纯思考无正文的 assistant 消息也留一条记录，reasoning 不许静默丢
    if (reasoning && m.role === "assistant") {
      push(m, m.parts.length - 1, {
        kind: "message",
        role: "assistant",
        summary: firstLine(reasoning),
        reasoning,
        ...(m.model ? { model: m.model } : {}),
        ...(m.stats ? { stats: m.stats } : {}),
      });
    }
  }
  return records;
}

/** turn = 一条 user 记录开启的段落（首个 user 之前的记录归为序幕 turn）。 */
export interface TrajectoryTurn {
  /** 段内首条记录的全表 index */
  startIndex: number;
  records: TrajectoryRecord[];
  steps: number;
  toolCalls: number;
  /** false = 序幕段（无 user 开头），折叠时不显示为首行 */
  headed: boolean;
}

export function groupTrajectoryTurns(records: TrajectoryRecord[]): TrajectoryTurn[] {
  const turns: TrajectoryTurn[] = [];
  let current: TrajectoryTurn | undefined;
  for (const record of records) {
    if (record.kind === "user" || !current) {
      current = {
        startIndex: record.index,
        records: [],
        steps: 0,
        toolCalls: 0,
        headed: record.kind === "user",
      };
      turns.push(current);
    }
    current.records.push(record);
    current.steps += 1;
    if (record.kind === "tool" || record.kind === "subtool") current.toolCalls += 1;
  }
  return turns;
}

/** 尾部优先分页：limit 条最新记录 + 是否还有更早页。 */
export function trajectoryTailWindow<T>(
  records: T[],
  limit: number,
): { window: T[]; hasEarlier: boolean } {
  const window = records.slice(Math.max(0, records.length - limit));
  return { window, hasEarlier: records.length > window.length };
}

/** 搜索：覆盖传入窗口内全部记录的摘要/正文/工具字段，大小写不敏感子串。 */
export function filterTrajectory(records: TrajectoryRecord[], query: string): TrajectoryRecord[] {
  const q = query.trim().toLowerCase();
  if (!q) return records;
  return records.filter((r) =>
    [
      r.summary,
      r.text,
      r.reasoning,
      r.source,
      r.tool?.name,
      r.tool?.call,
      r.tool?.args,
      r.tool?.result,
    ]
      .filter((x): x is string => Boolean(x))
      .some((x) => x.toLowerCase().includes(q)),
  );
}

/** Overview 时间线条：只有真实计时数据的记录才投影（计时不完整退化为单一颜色段）。 */
export interface OverviewBar {
  recordIndex: number;
  kind: "message" | "tool";
  start: number;
  end: number;
  /** assistant 条的首个非空 token 界限（TTFT）；缺省 = 未知，整段单一颜色 */
  ttftMs?: number | undefined;
  label: string;
}

export function overviewBars(records: TrajectoryRecord[]): OverviewBar[] {
  const bars: OverviewBar[] = [];
  for (const r of records) {
    if (r.kind === "tool" && r.tool) {
      const { startedAt, finishedAt } = r.tool;
      if (startedAt !== undefined && finishedAt !== undefined && finishedAt >= startedAt) {
        bars.push({
          recordIndex: r.index,
          kind: "tool",
          start: startedAt,
          end: finishedAt,
          label: r.tool.name,
        });
      }
      continue;
    }
    if (r.kind === "message" && r.stats) {
      // created_at 是收尾落盘时刻 ≈ run 结束；起点按实测 duration 回推
      const end = r.time;
      const start = end - r.stats.duration_ms;
      bars.push({
        recordIndex: r.index,
        kind: "message",
        start,
        end,
        ttftMs: r.stats.ttft_ms,
        label: r.model ? `${r.model.provider}/${r.model.model}` : "assistant",
      });
    }
  }
  return bars;
}

/** 拖选闭区间与记录（条）重叠判定：有条的按条，无条的按 time 点落在区间内。 */
export function recordsInRange(
  records: TrajectoryRecord[],
  bars: OverviewBar[],
  start: number,
  end: number,
): Set<number> {
  const byIndex = new Map(bars.map((b) => [b.recordIndex, b]));
  const hit = new Set<number>();
  for (const r of records) {
    const bar = byIndex.get(r.index);
    if (bar) {
      if (bar.start <= end && bar.end >= start) hit.add(r.index);
    } else if (r.time >= start && r.time <= end) {
      hit.add(r.index);
    }
  }
  return hit;
}
