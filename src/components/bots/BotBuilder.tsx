import { For, Show, createEffect, createSignal } from "solid-js";
import {
  botBuilderCancel,
  botBuilderGet,
  botBuilderGrant,
  botBuilderMessage,
  botBuilderStart,
  botBuilderTest,
  botPublish,
  botValidate,
  newBotId,
  type BuilderState,
} from "../../lib/bots";
import { flashErr, flashOk } from "../../lib/flash";
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
import BotBuilderStart from "./BotBuilderStart";

export default function BotBuilder(props: RefreshProps) {
  const [builder, setBuilder] = createSignal<BuilderState | null>(null);
  const [builderId, setBuilderId] = createSignal("");
  const [name, setName] = createSignal("");
  const [goal, setGoal] = createSignal("");
  const [message, setMessage] = createSignal("");
  const [grantReason, setGrantReason] = createSignal(
    "Owner reviewed the exact permission snapshot",
  );
  const [acting, setActing] = createSignal(false);
  const [loadErr, setLoadErr] = createSignal("");
  let loadSeq = 0;

  const reload = async () => {
    const id = builderId().trim();
    if (!id) return;
    const seq = ++loadSeq;
    try {
      const state = await botBuilderGet(id);
      if (seq !== loadSeq) return;
      setBuilder(state);
      setLoadErr("");
    } catch (error) {
      if (seq === loadSeq) setLoadErr(formatError(error));
    }
  };
  createEffect(() => {
    void props.epoch;
    if (builderId()) void reload();
  });

  const act = async (job: () => Promise<unknown>, label: string) => {
    if (acting()) return;
    setActing(true);
    try {
      await job();
      await reload();
      props.onChanged();
      flashOk(label);
    } catch (error) {
      flashErr(`${label}失败：${formatError(error)}`);
    } finally {
      setActing(false);
    }
  };
  const start = () => {
    const cleanGoal = goal().trim();
    if (!cleanGoal || !name().trim()) return;
    const botId = newBotId("bot");
    const sessionId = newBotId("builder");
    setBuilderId(sessionId);
    void act(async () => {
      await botBuilderStart(botId, sessionId, cleanGoal, name().trim(), newBotId("idem"));
      await botBuilderMessage(sessionId, newBotId("bmessage"), cleanGoal, newBotId("idem"));
    }, "Bot 草稿已生成");
  };
  const send = () => {
    const text = message().trim();
    const id = builderId();
    if (!text || !id) return;
    void act(async () => {
      await botBuilderMessage(id, newBotId("bmessage"), text, newBotId("idem"));
      setMessage("");
    }, "Bot 草稿已更新");
  };
  const grant = () => {
    const state = builder();
    if (!state?.draft) return;
    void act(
      () =>
        botBuilderGrant(
          state.builder_session_id,
          state.draft!.content_hash,
          grantReason().trim(),
          newBotId("idem"),
        ),
      "权限快照已授权",
    );
  };
  const test = () => {
    const state = builder();
    if (!state?.draft) return;
    void act(
      () => botBuilderTest(state.builder_session_id, newBotId("brun"), newBotId("idem")),
      "受控测试已排队",
    );
  };
  const validate = () => {
    const state = builder();
    if (!state?.draft) return;
    void act(() => botValidate(state.builder_session_id, newBotId("idem")), "确定性验证已完成");
  };
  const publish = () => {
    const state = builder();
    if (!state?.draft) return;
    void act(
      () => botPublish(state.builder_session_id, state.draft!.content_hash, newBotId("idem")),
      "Bot 已发布",
    );
  };
  const cancel = () => {
    const state = builder();
    if (!state) return;
    void act(
      () => botBuilderCancel(state.builder_session_id, newBotId("idem")),
      "Bot Build 已取消",
    );
  };
  const currentReport = () => {
    const hash = builder()?.draft?.content_hash;
    return builder()?.reports.findLast((report) => report.draft_hash === hash);
  };
  const hasGrant = () => {
    const hash = builder()?.draft?.content_hash;
    return builder()?.grants.some((grant) => grant.draft_hash === hash) ?? false;
  };
  const hasTest = () => {
    const hash = builder()?.draft?.content_hash;
    return builder()?.tests.some((test) => test.draft_hash === hash && test.passed) ?? false;
  };

  return (
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
      <BotBuilderStart
        name={name()}
        goal={goal()}
        builderId={builderId()}
        acting={acting()}
        loadErr={loadErr()}
        setName={setName}
        setGoal={setGoal}
        setBuilderId={setBuilderId}
        start={start}
        reload={() => void reload()}
      />

      <div class="lg:col-span-2 space-y-4">
        <Show
          when={builder()}
          fallback={
            <Panel title="Build Workspace">
              <p class="text-xs text-[var(--text-faint)]">
                创建或加载一个 Builder Session 后开始。
              </p>
            </Panel>
          }
        >
          {(state) => (
            <>
              <Panel
                title={state().draft?.definition.display_name || "Bot Build"}
                detail={`Session ${shortId(state().builder_session_id)}，状态 ${state().lifecycle}`}
              >
                <Show when={state().draft}>
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
                <div class="flex gap-2 mt-4">
                  <textarea
                    class={`${fieldClass} min-h-16`}
                    value={message()}
                    onInput={(event) => setMessage(event.currentTarget.value)}
                    placeholder="要求 Builder Agent 调整草稿"
                  />
                  <button
                    class={actionClass}
                    disabled={acting() || !message().trim()}
                    onClick={send}
                  >
                    更新
                  </button>
                </div>
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
                      state().active_test_run_id
                        ? `运行中 ${shortId(state().active_test_run_id!)}`
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
                    disabled={acting() || !state().draft || !grantReason().trim()}
                    onClick={grant}
                  >
                    授权当前权限
                  </button>
                </div>
                <div class="flex flex-wrap gap-2">
                  <button
                    class={actionClass}
                    disabled={acting() || !state().draft || Boolean(state().active_test_run_id)}
                    onClick={test}
                  >
                    运行受控测试
                  </button>
                  <button
                    class={actionClass}
                    disabled={acting() || !state().draft}
                    onClick={validate}
                  >
                    执行验证
                  </button>
                  <button
                    class={primaryClass}
                    disabled={acting() || !currentReport()?.publish_eligible}
                    onClick={publish}
                  >
                    发布 Bot
                  </button>
                  <button
                    class={actionClass}
                    disabled={acting() || state().lifecycle !== "active"}
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
          )}
        </Show>
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
