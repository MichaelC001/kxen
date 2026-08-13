import { Bot, Boxes, CalendarClock, Hammer, Play, ShieldAlert } from "lucide-solid";
import { For, Match, Switch, createSignal, onCleanup, onMount } from "solid-js";
import { client } from "../lib/client";
import { isTauri } from "../lib/runtime";
import { onDragStart } from "../lib/drag";
import { onTabKeyDown } from "../lib/tabs";
import BotLibrary from "../components/bots/BotLibrary";
import BotBuilder from "../components/bots/BotBuilder";
import BotCollaboration from "../components/bots/BotCollaboration";
import BotRoutines from "../components/bots/BotRoutines";
import BotRuns from "../components/bots/BotRuns";
import BotRecovery from "../components/bots/BotRecovery";
import type { BotBuilderTarget } from "../components/bots/shared";

type Tab = "library" | "build" | "collaboration" | "routines" | "runs" | "recovery";

const TABS: Array<{ id: Tab; label: string; icon: typeof Bot }> = [
  { id: "library", label: "Bot 管理", icon: Bot },
  { id: "build", label: "创建 Bot", icon: Hammer },
  { id: "collaboration", label: "Bot-to-Bot", icon: Boxes },
  { id: "routines", label: "Routine", icon: CalendarClock },
  { id: "runs", label: "Runs", icon: Play },
  { id: "recovery", label: "Recovery", icon: ShieldAlert },
];

export default function Bots() {
  const [tab, setTab] = createSignal<Tab>("library");
  const [epoch, setEpoch] = createSignal(0);
  const [builderTarget, setBuilderTarget] = createSignal<BotBuilderTarget>();
  let offStream: (() => void) | undefined;
  let offResync: (() => void) | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;
  let debounce: ReturnType<typeof setTimeout> | undefined;
  const refresh = () => setEpoch((value) => value + 1);
  const bump = () => {
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(refresh, 200);
  };

  onMount(() => {
    offStream = client.stream("bots").on(bump);
    offResync = client.onResync(refresh);
    timer = setInterval(refresh, 8000);
  });
  onCleanup(() => {
    offStream?.();
    offResync?.();
    if (timer) clearInterval(timer);
    if (debounce) clearTimeout(debounce);
  });

  return (
    <div class="h-full flex-1 overflow-auto">
      {isTauri() && <div class="h-8" data-tauri-drag-region onMouseDown={onDragStart} />}
      <div class="px-8 py-6 pt-2 max-w-7xl mx-auto">
        <div class="flex items-start gap-4 mb-5">
          <div>
            <h1 class="text-lg font-medium text-[var(--text)]">Bots</h1>
            <p class="text-xs text-[var(--text-faint)] mt-1">
              独立、持久化的重复工作单元。Bot Group 表示多个 Bot 协作，不是多人聊天。
            </p>
          </div>
        </div>
        <div
          class="flex flex-wrap gap-1 border-b border-[var(--border)] mb-5"
          role="tablist"
          aria-label="Bot 功能"
          aria-orientation="horizontal"
        >
          <For each={TABS}>
            {(item) => (
              <button
                id={`bots-tab-${item.id}`}
                role="tab"
                aria-selected={tab() === item.id}
                aria-controls={`bots-panel-${item.id}`}
                tabIndex={tab() === item.id ? 0 : -1}
                class="pressable px-3 py-2 text-xs flex items-center gap-1.5 border-b-2 -mb-px"
                classList={{
                  "border-[var(--accent)] text-[var(--text)]": tab() === item.id,
                  "border-transparent text-[var(--text-dim)]": tab() !== item.id,
                }}
                onClick={() => setTab(item.id)}
                onKeyDown={onTabKeyDown}
              >
                <item.icon size={13} />
                {item.label}
              </button>
            )}
          </For>
        </div>
        <div id={`bots-panel-${tab()}`} role="tabpanel" aria-labelledby={`bots-tab-${tab()}`}>
          <Switch>
            <Match when={tab() === "library"}>
              <BotLibrary
                epoch={epoch()}
                onChanged={refresh}
                onBuild={(target) => {
                  setBuilderTarget(target);
                  setTab("build");
                }}
              />
            </Match>
            <Match when={tab() === "build"}>
              <BotBuilder
                epoch={epoch()}
                onChanged={refresh}
                target={builderTarget()}
                onClearTarget={() => setBuilderTarget(undefined)}
              />
            </Match>
            <Match when={tab() === "collaboration"}>
              <BotCollaboration epoch={epoch()} onChanged={refresh} />
            </Match>
            <Match when={tab() === "routines"}>
              <BotRoutines epoch={epoch()} onChanged={refresh} />
            </Match>
            <Match when={tab() === "runs"}>
              <BotRuns epoch={epoch()} onChanged={refresh} />
            </Match>
            <Match when={tab() === "recovery"}>
              <BotRecovery epoch={epoch()} onChanged={refresh} />
            </Match>
          </Switch>
        </div>
      </div>
    </div>
  );
}
