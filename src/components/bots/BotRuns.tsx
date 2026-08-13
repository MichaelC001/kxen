import { For, Show, createEffect, createSignal } from "solid-js";
import {
  botArtifactGet,
  botArtifactRestore,
  botArtifactTrash,
  botRunApproval,
  botRunCancel,
  botRunInput,
  botRunList,
  newBotId,
  type BotRun,
} from "../../lib/bots";
import { createReconciledMutation } from "../../lib/async-guard";
import { flashErr } from "../../lib/flash";
import { formatError } from "../../lib/error-text";
import {
  actionClass,
  fieldClass,
  Panel,
  primaryClass,
  shortId,
  statusClass,
  type RefreshProps,
} from "./shared";
import { runCancellationApplied } from "./mutation-state";
import { decodeArtifactPreview } from "./artifact-preview";
import { BotRunMetric, terminalRunStatus } from "./BotRunMetric";

export default function BotRuns(props: RefreshProps) {
  const [runs, setRuns] = createSignal<BotRun[]>([]);
  const [selectedId, setSelectedId] = createSignal("");
  const [input, setInput] = createSignal("");
  const [previewing, setPreviewing] = createSignal(false);
  const [artifactPreview, setArtifactPreview] = createSignal<{
    id: string;
    name: string;
    content: string;
  } | null>(null);
  const [loadErr, setLoadErr] = createSignal("");
  let loadSeq = 0;
  const reload = async () => {
    const seq = ++loadSeq;
    try {
      const items = await botRunList();
      if (seq !== loadSeq) return;
      items.sort((left, right) => right.updated_at_ms - left.updated_at_ms);
      setRuns(items);
      const selected = selectedId();
      if (!items.some((run) => run.spec.run_id === selected))
        setSelectedId(items[0]?.spec.run_id ?? "");
      setLoadErr("");
    } catch (error) {
      if (seq === loadSeq) setLoadErr(formatError(error));
    }
  };
  createEffect(() => {
    void props.epoch;
    void reload();
  });
  const selected = () => runs().find((run) => run.spec.run_id === selectedId()) || null;
  const mutation = createReconciledMutation({ refresh: reload, onChanged: props.onChanged });
  const acting = () => mutation.pending() || previewing();
  const approval = (allow: boolean) => {
    const run = selected();
    if (!run?.approval) return;
    const runId = run.spec.run_id;
    const approvalId = run.approval.approval_id;
    void mutation.run({
      key: `run:${runId}:approval:${approvalId}:${allow}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => {
        const current = selected();
        if (!current || current.spec.run_id !== runId || !current.approval)
          throw new Error("Run approval is no longer available");
        return botRunApproval(runId, approvalId, allow, current.event_version, idempotencyKey);
      },
      applied: () => selected()?.spec.run_id === runId && !selected()?.approval,
      okText: allow ? "审批已允许" : "审批已拒绝",
    });
  };
  const provideInput = () => {
    const run = selected();
    const text = input().trim();
    if (!run?.input_request || !text) return;
    const runId = run.spec.run_id;
    const requestId = run.input_request.request_id;
    void mutation.run({
      key: `run:${runId}:input:${requestId}:${text}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => {
        const current = selected();
        if (!current || current.spec.run_id !== runId || !current.input_request)
          throw new Error("Run input request is no longer available");
        return botRunInput(
          runId,
          requestId,
          [{ kind: "text", text }],
          current.event_version,
          idempotencyKey,
        );
      },
      applied: () =>
        selected()?.spec.run_id === runId && selected()?.input_request?.request_id !== requestId,
      onApplied: () => setInput(""),
      okText: "输入已绑定",
    });
  };
  const cancel = () => {
    const run = selected();
    if (!run || terminalRunStatus(run.status)) return;
    const runId = run.spec.run_id;
    void mutation.run({
      key: `run:${runId}:cancel`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => {
        const current = selected();
        if (!current || current.spec.run_id !== runId) throw new Error("selected BotRun changed");
        return botRunCancel(runId, current.event_version, idempotencyKey, "canceled by owner");
      },
      applied: () => runCancellationApplied(selected(), runId),
      okText: "BotRun 取消请求已记录",
    });
  };
  const inspectArtifact = async (artifactId: string, displayName: string, mediaType: string) => {
    if (acting()) return;
    setPreviewing(true);
    try {
      const payload = (await botArtifactGet(artifactId)) as { content_base64: string };
      const content = decodeArtifactPreview(payload.content_base64, mediaType);
      setArtifactPreview({ id: artifactId, name: displayName, content });
    } catch (error) {
      flashErr(`读取 Artifact 失败：${formatError(error)}`);
    } finally {
      setPreviewing(false);
    }
  };
  const trashArtifact = (artifactId: string) => {
    void mutation.run({
      key: `artifact:${artifactId}:trash`,
      prepare: () => ({}),
      execute: () => botArtifactTrash(artifactId),
      onApplied: () => {
        if (artifactPreview()?.id === artifactId) setArtifactPreview(null);
      },
      okText: "Artifact 已移到废纸篓",
    });
  };
  const restoreArtifact = (artifactId: string) => {
    void mutation.run({
      key: `artifact:${artifactId}:restore`,
      prepare: () => ({}),
      execute: () => botArtifactRestore(artifactId),
      okText: "Artifact 已恢复",
    });
  };

  return (
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
      <Panel title="BotRuns" detail="Run 是可恢复的执行真源，瞬态 delta 只用于渲染。">
        <Show when={loadErr()}>
          <p class="text-xs text-[var(--err)] mb-2">{loadErr()}</p>
        </Show>
        <div class="space-y-2 max-h-[65vh] overflow-auto">
          <For
            each={runs()}
            fallback={<p class="text-xs text-[var(--text-faint)]">暂无 BotRun。</p>}
          >
            {(run) => (
              <button
                class="pressable w-full text-left rounded border border-[var(--border)] p-2"
                classList={{ "border-[var(--accent)]": selectedId() === run.spec.run_id }}
                onClick={() => setSelectedId(run.spec.run_id)}
              >
                <div class="flex gap-2">
                  <span class="text-xs truncate">{run.spec.bot_id}</span>
                  <span class={`ml-auto text-2xs ${statusClass(run.status)}`}>{run.status}</span>
                </div>
                <div class="text-2xs font-mono text-[var(--text-faint)]">
                  {shortId(run.spec.run_id)}
                </div>
              </button>
            )}
          </For>
        </div>
      </Panel>
      <div class="lg:col-span-2 space-y-4">
        <Show
          when={selected()}
          fallback={
            <Panel title="Run 详情">
              <p class="text-xs text-[var(--text-faint)]">选择一个 Run。</p>
            </Panel>
          }
        >
          {(run) => (
            <>
              <Panel
                title={`Run ${shortId(run().spec.run_id)}`}
                detail={`${run().spec.trigger.kind}，revision ${shortId(run().spec.revision_id)}`}
              >
                <div class="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs">
                  <BotRunMetric
                    label="状态"
                    value={run().status}
                    tone={statusClass(run().status)}
                  />
                  <BotRunMetric
                    label="Tokens"
                    value={String(run().usage.input_tokens + run().usage.output_tokens)}
                  />
                  <BotRunMetric label="Tool calls" value={String(run().usage.tool_calls)} />
                  <BotRunMetric label="Turns" value={String(run().usage.turns)} />
                </div>
                <Show when={run().error_message}>
                  <div class="mt-3 rounded border border-[var(--err)]/50 p-2 text-xs text-[var(--err)] selectable">
                    {run().error_code}: {run().error_message}
                  </div>
                </Show>
                <Show when={!terminalRunStatus(run().status)}>
                  <button class={`${actionClass} mt-3`} disabled={acting()} onClick={cancel}>
                    Cancel Run
                  </button>
                </Show>
              </Panel>

              <Show when={run().approval}>
                {(approvalRequest) => (
                  <Panel
                    title="需要审批"
                    detail="受控副作用审批绑定精确 operation_id，协作任务审批不伪造副作用 identity。"
                  >
                    <p class="text-xs selectable mb-3">{approvalRequest().summary}</p>
                    <div class="flex gap-2">
                      <button
                        class={primaryClass}
                        disabled={acting()}
                        onClick={() => approval(true)}
                      >
                        Allow
                      </button>
                      <button
                        class={actionClass}
                        disabled={acting()}
                        onClick={() => approval(false)}
                      >
                        Deny
                      </button>
                    </div>
                  </Panel>
                )}
              </Show>
              <Show when={run().input_request}>
                {(request) => (
                  <Panel title="需要输入" detail={request().prompt}>
                    <div class="flex gap-2">
                      <input
                        class={fieldClass}
                        value={input()}
                        onInput={(event) => setInput(event.currentTarget.value)}
                        placeholder="补充本次 Run 所需信息"
                      />
                      <button
                        class={primaryClass}
                        disabled={acting() || !input().trim()}
                        onClick={provideInput}
                      >
                        提交
                      </button>
                    </div>
                  </Panel>
                )}
              </Show>

              <Panel
                title="结果与 Artifacts"
                detail="终态结果和 Artifact manifest 可在重启后重新读取。"
              >
                <div class="space-y-2 selectable">
                  <For
                    each={run().result}
                    fallback={<p class="text-xs text-[var(--text-faint)]">尚无终态结果。</p>}
                  >
                    {(part) => (
                      <div class="rounded border border-[var(--border)] p-2 text-xs whitespace-pre-wrap">
                        {part.kind === "text" ? part.text : JSON.stringify(part, null, 2)}
                      </div>
                    )}
                  </For>
                  <For each={run().artifacts}>
                    {(artifact) => (
                      <div class="rounded border border-[var(--border)] p-2 text-xs">
                        <div>{artifact.display_name}</div>
                        <div class="text-2xs text-[var(--text-faint)]">
                          {artifact.media_type}，{artifact.size_bytes} bytes
                        </div>
                        <div class="font-mono text-2xs break-all">{artifact.content_hash}</div>
                        <div class="flex flex-wrap gap-2 mt-2">
                          <button
                            class={actionClass}
                            disabled={acting()}
                            onClick={() =>
                              void inspectArtifact(
                                artifact.artifact_id,
                                artifact.display_name,
                                artifact.media_type,
                              )
                            }
                          >
                            验证并预览
                          </button>
                          <button
                            class={actionClass}
                            disabled={acting()}
                            onClick={() => trashArtifact(artifact.artifact_id)}
                          >
                            Trash
                          </button>
                          <button
                            class={actionClass}
                            disabled={acting()}
                            onClick={() => restoreArtifact(artifact.artifact_id)}
                          >
                            Restore
                          </button>
                        </div>
                      </div>
                    )}
                  </For>
                  <Show when={artifactPreview()}>
                    {(preview) => (
                      <div class="rounded border border-[var(--accent)]/60 p-3 text-xs">
                        <div class="mb-2 text-[var(--accent-hover)]">{preview().name}</div>
                        <pre class="selectable whitespace-pre-wrap max-h-80 overflow-auto">
                          {preview().content}
                        </pre>
                      </div>
                    )}
                  </Show>
                </div>
              </Panel>
            </>
          )}
        </Show>
      </div>
    </div>
  );
}
