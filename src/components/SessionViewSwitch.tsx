// Chat / Trajectory 双视图并存：标签栏 + 两个视图的显隐切换。
// Chat 保持挂载（仅 display 隐藏），滚动锚点由 lib/session-view 存取，切回原地恢复。
// Trajectory 按需分包：首次切到该标签才加载 chunk，此后保持挂载。
import {
  For,
  Show,
  Suspense,
  createEffect,
  createSignal,
  lazy,
  type Accessor,
  type JSX,
} from "solid-js";
import {
  consumeInspectTarget,
  inspectTarget,
  sessionView,
  switchSessionView,
  type SessionView as View,
} from "../lib/session-view";

const TrajectoryView = lazy(() => import("./TrajectoryView"));

const TABS: { id: View; label: string }[] = [
  { id: "chat", label: "Chat" },
  { id: "trajectory", label: "Trajectory" },
];

export default function SessionViewSwitch(props: {
  sessionId: Accessor<string>;
  streaming: Accessor<boolean>;
  children: JSX.Element;
}) {
  const [trajectoryMounted, setTrajectoryMounted] = createSignal(false);
  createEffect(() => {
    if (sessionView() === "trajectory") setTrajectoryMounted(true);
  });
  return (
    <>
      <Show when={props.sessionId() !== ""}>
        <div class="flex gap-1 px-4 pt-2">
          <For each={TABS}>
            {(tab) => (
              <button
                data-testid={`view-tab-${tab.id}`}
                class="pressable px-2.5 py-1 rounded-t text-xs border border-b-0 border-[var(--border)]"
                classList={{
                  "bg-[var(--bg-raised)] text-[var(--text)]": sessionView() === tab.id,
                  "text-[var(--text-dim)]": sessionView() !== tab.id,
                }}
                onClick={() => switchSessionView(tab.id)}
              >
                {tab.label}
              </button>
            )}
          </For>
        </div>
      </Show>
      <div class={sessionView() === "chat" ? "flex-1 min-h-0 flex flex-col relative" : "hidden"}>
        {props.children}
      </div>
      <div class={sessionView() === "trajectory" ? "flex-1 min-h-0 flex flex-col" : "hidden"}>
        <Show when={trajectoryMounted()}>
          <Suspense>
            <TrajectoryView
              sessionId={props.sessionId}
              active={() => sessionView() === "trajectory"}
              streaming={props.streaming}
              focus={inspectTarget}
              onFocusConsumed={consumeInspectTarget}
            />
          </Suspense>
        </Show>
      </div>
    </>
  );
}
