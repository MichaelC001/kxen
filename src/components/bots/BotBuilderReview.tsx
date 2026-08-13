import { For, Show, createSignal } from "solid-js";
import {
  botBuilderCancel,
  botBuilderGrant,
  botBuilderTest,
  botPublish,
  botValidate,
  newBotId,
  type BuilderState,
} from "../../lib/bots";
import type { ReconciledMutationController } from "../../lib/async-guard";
import { actionClass, fieldClass, Panel, primaryClass, shortId, statusClass } from "./shared";

export default function BotBuilderReview(props: {
  state: BuilderState;
  mutation: ReconciledMutationController;
}) {
  const [grantReason, setGrantReason] = createSignal("");
  const currentReport = () => {
    const hash = props.state.draft?.content_hash;
    return props.state.reports.findLast((report) => report.draft_hash === hash);
  };
  const hasGrant = () => {
    const hash = props.state.draft?.content_hash;
    return props.state.grants.some((grant) => grant.draft_hash === hash);
  };
  const hasTest = () => {
    const hash = props.state.draft?.content_hash;
    return props.state.tests.some((test) => test.draft_hash === hash && test.passed);
  };
  const grant = () => {
    const draft = props.state.draft;
    if (!draft) return;
    const sessionId = props.state.builder_session_id;
    const reason = grantReason().trim();
    void props.mutation.run({
      key: `builder:${sessionId}:grant:${draft.content_hash}:${reason}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) =>
        botBuilderGrant(sessionId, draft.content_hash, reason, idempotencyKey),
      applied: () => props.state.grants.some((grant) => grant.draft_hash === draft.content_hash),
      okText: "权限快照已授权",
    });
  };
  const test = () => {
    const draft = props.state.draft;
    if (!draft) return;
    const sessionId = props.state.builder_session_id;
    void props.mutation.run({
      key: `builder:${sessionId}:test:${draft.content_hash}`,
      prepare: () => ({ runId: newBotId("brun"), idempotencyKey: newBotId("idem") }),
      execute: ({ runId, idempotencyKey }) => botBuilderTest(sessionId, runId, idempotencyKey),
      applied: ({ runId }) =>
        props.state.active_test_run_id === runId ||
        props.state.tests.some((test) => test.run_id === runId),
      okText: "受控测试已排队",
    });
  };
  const validate = () => {
    const draft = props.state.draft;
    if (!draft) return;
    const sessionId = props.state.builder_session_id;
    void props.mutation.run({
      key: `builder:${sessionId}:validate:${draft.content_hash}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => botValidate(sessionId, idempotencyKey),
      applied: () => props.state.reports.some((report) => report.draft_hash === draft.content_hash),
      okText: "确定性验证已完成",
    });
  };
  const publish = () => {
    const draft = props.state.draft;
    if (!draft) return;
    const sessionId = props.state.builder_session_id;
    void props.mutation.run({
      key: `builder:${sessionId}:publish:${draft.content_hash}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => botPublish(sessionId, draft.content_hash, idempotencyKey),
      okText: "Bot 已发布",
    });
  };
  const cancel = () => {
    const sessionId = props.state.builder_session_id;
    void props.mutation.run({
      key: `builder:${sessionId}:cancel`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => botBuilderCancel(sessionId, idempotencyKey),
      applied: () => props.state.lifecycle === "canceled",
      okText: "构建对话已取消",
    });
  };

  return (
    <>
      <Panel
        title={props.state.draft?.definition.display_name || "Bot 构建"}
        detail={`Session ${shortId(props.state.builder_session_id)}，状态 ${props.state.lifecycle}`}
      >
        <Show when={props.state.draft}>
          {(draft) => (
            <div class="space-y-3 text-xs">
              <div>
                <span class="text-[var(--text-faint)]">目标</span>
                <p class="selectable whitespace-pre-wrap">{draft().definition.objective}</p>
              </div>
              <div>
                <span class="text-[var(--text-faint)]">Instructions</span>
                <p class="selectable whitespace-pre-wrap max-h-40 overflow-auto">
                  {draft().definition.instructions}
                </p>
              </div>
              <div>
                <span class="text-[var(--text-faint)]">Success criteria</span>
                <ul class="list-disc pl-5">
                  <For each={draft().definition.success_criteria}>
                    {(criterion) => <li>{criterion}</li>}
                  </For>
                </ul>
              </div>
              <div>
                <span class="text-[var(--text-faint)]">Capabilities</span>
                <p>{draft().definition.capabilities.join(", ") || "无"}</p>
              </div>
              <div>
                <span class="text-[var(--text-faint)]">Workspace grants</span>
                <div class="space-y-1">
                  <For each={draft().definition.resources.workspaces} fallback={<p>无</p>}>
                    {(workspace) => (
                      <div class="rounded border border-[var(--border)] p-2">
                        <div class="font-mono break-all">{workspace.workspace_id}</div>
                        <For
                          each={workspace.paths}
                          fallback={<div class="text-[var(--text-faint)]">无路径授权</div>}
                        >
                          {(path) => (
                            <div class="font-mono text-2xs">
                              {path.access} {path.relative_path}
                            </div>
                          )}
                        </For>
                      </div>
                    )}
                  </For>
                </div>
              </div>
              <div>
                <span class="text-[var(--text-faint)]">Connectors</span>
                <p>{draft().definition.resources.connectors.join(", ") || "无"}</p>
              </div>
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <DefinitionValues
                  label="Input contract"
                  values={draft().definition.input_contract}
                />
                <DefinitionValues
                  label="Output contract"
                  values={draft().definition.output_contract}
                />
                <DefinitionValues label="Budget" values={draft().definition.budget} />
                <DefinitionValues label="Context" values={draft().definition.context} />
                <DefinitionValues label="Memory" values={draft().definition.memory} />
                <DefinitionValues label="Failure policy" values={draft().definition.failure} />
              </div>
              <div>
                <span class="text-[var(--text-faint)]">Execution policy</span>
                <p>
                  MRM role {draft().definition.mrm_role}，Approval {draft().definition.approval}
                </p>
              </div>
              <div>
                <span class="text-[var(--text-faint)]">Bot-to-Bot policy</span>
                <p>
                  Direct {draft().definition.communication.allow_direct ? "允许" : "禁止"}，Group{" "}
                  {draft().definition.communication.allow_groups ? "允许" : "禁止"}，Peers{" "}
                  {draft().definition.communication.allowed_peers.join(", ") || "无"}
                </p>
              </div>
              <div>
                <span class="text-[var(--text-faint)]">Draft hash</span>
                <p class="font-mono text-2xs break-all">{draft().content_hash}</p>
              </div>
            </div>
          )}
        </Show>
      </Panel>

      <Panel
        title="发布门禁"
        detail="必须依次绑定同一个 draft hash 的 Owner 权限授权、受控测试证据和确定性 PASS 报告。"
      >
        <div class="grid grid-cols-1 md:grid-cols-3 gap-3 text-xs mb-3">
          <Gate
            label="权限授权"
            passed={hasGrant()}
            detail={hasGrant() ? "已绑定当前草稿" : "等待 Owner 确认"}
          />
          <Gate
            label="受控测试"
            passed={hasTest()}
            detail={
              props.state.active_test_run_id
                ? `运行中 ${shortId(props.state.active_test_run_id)}`
                : hasTest()
                  ? "PASS"
                  : "尚未通过"
            }
          />
          <Gate
            label="确定性验证"
            passed={Boolean(currentReport()?.publish_eligible)}
            detail={currentReport()?.publish_eligible ? "PASS" : "尚未满足发布条件"}
          />
        </div>
        <div class="flex gap-2 mb-3">
          <input
            class={fieldClass}
            value={grantReason()}
            onInput={(event) => setGrantReason(event.currentTarget.value)}
            placeholder="说明为何授权这组能力和资源"
          />
          <button
            class={actionClass}
            disabled={props.mutation.pending() || !props.state.draft || !grantReason().trim()}
            onClick={grant}
          >
            授权当前权限
          </button>
        </div>
        <div class="flex flex-wrap gap-2">
          <button
            class={actionClass}
            disabled={
              props.mutation.pending() ||
              !props.state.draft ||
              Boolean(props.state.active_test_run_id)
            }
            onClick={test}
          >
            运行受控测试
          </button>
          <button
            class={actionClass}
            disabled={props.mutation.pending() || !props.state.draft}
            onClick={validate}
          >
            执行验证
          </button>
          <button
            class={primaryClass}
            disabled={props.mutation.pending() || !currentReport()?.publish_eligible}
            onClick={publish}
          >
            发布 Bot
          </button>
          <button
            class={actionClass}
            disabled={props.mutation.pending() || props.state.lifecycle !== "active"}
            onClick={cancel}
          >
            取消构建对话
          </button>
        </div>
        <Show when={currentReport()}>
          {(report) => (
            <div class="mt-4 space-y-1">
              <For each={report().findings}>
                {(finding) => (
                  <div class="text-xs flex gap-2">
                    <span class={statusClass(finding.status)}>{finding.status}</span>
                    <span class="font-mono">{finding.code}</span>
                    <span class="text-[var(--text-dim)]">{finding.message}</span>
                  </div>
                )}
              </For>
            </div>
          )}
        </Show>
      </Panel>
    </>
  );
}

function DefinitionValues(props: { label: string; values: object }) {
  const entries = () =>
    Object.entries(props.values).filter(([, value]) => value !== null && value !== undefined);
  return (
    <div>
      <span class="text-[var(--text-faint)]">{props.label}</span>
      <div class="font-mono text-2xs break-words">
        {entries()
          .map(
            ([key, value]) =>
              `${key}=${Array.isArray(value) ? value.join(",") || "无" : String(value)}`,
          )
          .join("，") || "无"}
      </div>
    </div>
  );
}

function Gate(props: { label: string; passed: boolean; detail: string }) {
  return (
    <div class="rounded border border-[var(--border)] p-2">
      <div class={props.passed ? "text-[var(--ok)]" : "text-[var(--warn)]"}>
        {props.passed ? "PASS" : "WAIT"} {props.label}
      </div>
      <div class="text-2xs text-[var(--text-faint)] mt-1">{props.detail}</div>
    </div>
  );
}
