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
import { actionClass, fieldClass, Panel, primaryClass, shortId, statusClass } from "./shared";

export default function BotBuilderReview(props: {
  state: BuilderState;
  acting: boolean;
  act: (job: () => Promise<unknown>, label: string) => Promise<void>;
}) {
  const [grantReason, setGrantReason] = createSignal(
    "Owner reviewed the exact permission snapshot",
  );
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
    void props.act(
      () =>
        botBuilderGrant(
          props.state.builder_session_id,
          draft.content_hash,
          grantReason().trim(),
          newBotId("idem"),
        ),
      "权限快照已授权",
    );
  };
  const test = () => {
    if (!props.state.draft) return;
    void props.act(
      () => botBuilderTest(props.state.builder_session_id, newBotId("brun"), newBotId("idem")),
      "受控测试已排队",
    );
  };
  const validate = () => {
    if (!props.state.draft) return;
    void props.act(
      () => botValidate(props.state.builder_session_id, newBotId("idem")),
      "确定性验证已完成",
    );
  };
  const publish = () => {
    const draft = props.state.draft;
    if (!draft) return;
    void props.act(
      () => botPublish(props.state.builder_session_id, draft.content_hash, newBotId("idem")),
      "Bot 已发布",
    );
  };
  const cancel = () => {
    void props.act(
      () => botBuilderCancel(props.state.builder_session_id, newBotId("idem")),
      "Bot Build 已取消",
    );
  };

  return (
    <>
      <Panel
        title={props.state.draft?.definition.display_name || "Bot Build"}
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
            placeholder="授权理由"
          />
          <button
            class={actionClass}
            disabled={props.acting || !props.state.draft || !grantReason().trim()}
            onClick={grant}
          >
            授权当前权限
          </button>
        </div>
        <div class="flex flex-wrap gap-2">
          <button
            class={actionClass}
            disabled={props.acting || !props.state.draft || Boolean(props.state.active_test_run_id)}
            onClick={test}
          >
            运行受控测试
          </button>
          <button
            class={actionClass}
            disabled={props.acting || !props.state.draft}
            onClick={validate}
          >
            执行验证
          </button>
          <button
            class={primaryClass}
            disabled={props.acting || !currentReport()?.publish_eligible}
            onClick={publish}
          >
            发布 Bot
          </button>
          <button
            class={actionClass}
            disabled={props.acting || props.state.lifecycle !== "active"}
            onClick={cancel}
          >
            取消 Build
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
