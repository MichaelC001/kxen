// Trajectory 局部侧栏检查器：选中记录的标签页随类型变化。
// 消息类：内容 / 来源 / 模型；工具类：Summary / Payload / Result / Schema / Timing。
// 未持久化的字段（schema 快照、缺计时的起止）一律显式标注「未知」，不编造。
import { createSignal, For, Show, type Accessor } from "solid-js";
import { X } from "lucide-solid";
import Markdown from "./Markdown";
import { formatClock, formatMs, toolDurationMs, type TrajectoryRecord } from "../lib/trajectory";

const UNKNOWN = "未知";

function Field(props: { label: string; value?: string | number | undefined }) {
  return (
    <div class="flex gap-2 text-xs py-0.5">
      <span class="text-[var(--text-faint)] shrink-0 w-20">{props.label}</span>
      <span class="selectable text-[var(--text-dim)] break-all">
        {props.value === undefined || props.value === "" ? UNKNOWN : props.value}
      </span>
    </div>
  );
}

function Pre(props: { text?: string | undefined }) {
  return (
    <Show
      when={props.text !== undefined && props.text !== ""}
      fallback={<div class="text-xs text-[var(--text-faint)]">{UNKNOWN}</div>}
    >
      <pre class="selectable text-xs text-[var(--text-dim)] whitespace-pre-wrap break-all max-h-72 overflow-auto">
        {props.text}
      </pre>
    </Show>
  );
}

function Images(props: { images: { media_type: string; data: string }[] }) {
  return (
    <For each={props.images}>
      {(img) => (
        <img
          src={`data:${img.media_type};base64,${img.data}`}
          alt="消息图片"
          class="max-w-full rounded border border-[var(--border)]"
        />
      )}
    </For>
  );
}

function TimingTab(props: { record: TrajectoryRecord }) {
  const tool = () => props.record.tool;
  const duration = () => {
    const t = tool();
    return t ? toolDurationMs(t) : undefined;
  };
  const stats = () => props.record.stats;
  return (
    <div>
      <Field label="落盘时刻" value={formatClock(props.record.time)} />
      <Field
        label="开始"
        value={tool()?.startedAt !== undefined ? formatClock(tool()!.startedAt!) : undefined}
      />
      <Field
        label="结束"
        value={tool()?.finishedAt !== undefined ? formatClock(tool()!.finishedAt!) : undefined}
      />
      <Field label="耗时" value={duration() !== undefined ? formatMs(duration()!) : undefined} />
      <Field label="TTFT" value={stats() ? formatMs(stats()!.ttft_ms) : undefined} />
      <Field
        label="解码"
        value={stats() ? formatMs(Math.max(0, stats()!.duration_ms - stats()!.ttft_ms)) : undefined}
      />
      <Field
        label="tokens"
        value={
          stats()
            ? `in ${stats()!.input_tokens} / out ${stats()!.output_tokens}${stats()!.usage_complete === false ? "（计量不完整，为已知下限）" : ""}`
            : undefined
        }
      />
    </div>
  );
}

export default function TrajectoryInspector(props: {
  record: Accessor<TrajectoryRecord | undefined>;
  onClose: () => void;
}) {
  const [tab, setTab] = createSignal("");
  const tabs = () => {
    const r = props.record();
    if (!r) return [] as string[];
    return r.kind === "tool" || r.kind === "subtool"
      ? ["Summary", "Payload", "Result", "Schema", "Timing"]
      : ["内容", "来源", "模型"];
  };
  const activeTab = () => {
    const available = tabs();
    return available.includes(tab()) ? tab() : (available[0] ?? "");
  };
  return (
    <Show when={props.record()}>
      {(record) => (
        <aside
          data-testid="trajectory-inspector"
          class="w-80 shrink-0 border-l border-[var(--border)] bg-[var(--bg-raised)] flex flex-col min-h-0"
        >
          <div class="flex items-center gap-2 px-3 py-2 border-b border-[var(--border)]">
            <span class="text-xs font-medium text-[var(--text)] flex-1 truncate">
              #{record().index} {record().kind}
            </span>
            <button
              class="pressable text-[var(--text-faint)]"
              title="关闭检查器"
              onClick={props.onClose}
            >
              <X size={13} />
            </button>
          </div>
          <div class="flex gap-1 px-3 pt-2">
            <For each={tabs()}>
              {(name) => (
                <button
                  class="pressable px-2 py-0.5 rounded text-2xs border border-[var(--border)]"
                  classList={{
                    "bg-[var(--bg-overlay)] text-[var(--text)]": activeTab() === name,
                    "text-[var(--text-dim)]": activeTab() !== name,
                  }}
                  onClick={() => setTab(name)}
                >
                  {name}
                </button>
              )}
            </For>
          </div>
          <div class="flex-1 min-h-0 overflow-auto px-3 py-2">
            <Show when={activeTab() === "内容" || activeTab() === "Summary"}>
              <Show when={record().text !== undefined}>
                <Markdown text={record().text ?? ""} />
              </Show>
              <Show when={record().text === undefined}>
                <Pre text={record().tool?.call ?? record().summary} />
              </Show>
              <Show when={record().reasoning}>
                <div class="mt-2 text-2xs text-[var(--text-faint)]">reasoning</div>
                <Pre text={record().reasoning} />
              </Show>
              <Show when={record().images?.length}>
                {<Images images={record().images ?? []} />}
              </Show>
            </Show>
            <Show when={activeTab() === "Payload"}>
              <Pre text={record().tool?.args} />
            </Show>
            <Show when={activeTab() === "Result"}>
              <Pre text={record().tool?.result} />
            </Show>
            <Show when={activeTab() === "Schema"}>
              {/* schema 快照未落盘：这里展示 UNKNOWN，不回放当前工具表冒充历史 */}
              <div class="text-xs text-[var(--text-faint)]">
                未知（该版本未落盘调用发生时的 schema 快照）
              </div>
            </Show>
            <Show when={activeTab() === "Timing"}>
              <TimingTab record={record()} />
            </Show>
            <Show when={activeTab() === "来源"}>
              <Field label="message id" value={record().messageId} />
              <Field label="part" value={record().partIndex} />
              <Field label="role" value={record().role} />
              <Field label="来源" value={record().source} />
              <Show when={record().contextItems?.length}>
                <div class="mt-1 text-2xs text-[var(--text-faint)]">上下文引用</div>
                <Pre text={JSON.stringify(record().contextItems, null, 2)} />
              </Show>
            </Show>
            <Show when={activeTab() === "模型"}>
              <Field label="provider" value={record().model?.provider} />
              <Field label="model" value={record().model?.model} />
              <Field label="account" value={record().model?.account ?? undefined} />
            </Show>
          </div>
        </aside>
      )}
    </Show>
  );
}
