// 会话行：活动点 / 标题(双击重命名) / 相对时间 / hover 操作（置顶、重命名、删除带确认）。
import { createSignal, Show } from "solid-js";
import { Check, Pin, PinOff, X } from "lucide-solid";
import { sessionUpdateMeta, type SessionMeta } from "../lib/chat";
import { openMenu } from "../lib/context-menu";
import { relTime } from "../lib/time";
import { activeSessionId } from "../lib/state";

export default function SessionRow(props: {
  session: SessionMeta;
  onOpen: () => void;
  onDelete: () => void;
  onChanged: () => void;
  draggable: boolean;
  onDragStart: (e: DragEvent) => void;
  onDragOver: (e: DragEvent) => void;
  onDrop: (e: DragEvent) => void;
}) {
  const [renaming, setRenaming] = createSignal(false);
  const [confirming, setConfirming] = createSignal(false);
  const [draft, setDraft] = createSignal("");
  let inputRef: HTMLInputElement | undefined;

  const s = () => props.session;

  const commitRename = async () => {
    const t = draft().trim();
    if (t && t !== s().title) {
      await sessionUpdateMeta(s().id, { title: t });
      props.onChanged();
    }
    setRenaming(false);
  };

  const togglePin = async (e: MouseEvent) => {
    e.stopPropagation();
    await sessionUpdateMeta(s().id, { pinned: !s().pinned });
    props.onChanged();
  };

  return (
    <div
      class="interactive group relative flex items-center rounded-md text-sm cursor-pointer"
      classList={{
        "bg-[var(--bg-overlay)] text-[var(--text)]": s().id === activeSessionId(),
        "text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60": s().id !== activeSessionId(),
      }}
      draggable={props.draggable && !renaming()}
      onClick={props.onOpen}
      onContextMenu={(e) => {
        openMenu(e, [
          {
            label: "重命名",
            action: () => {
              setDraft(s().title);
              setRenaming(true);
              setTimeout(() => inputRef?.select(), 0);
            },
          },
          {
            label: s().pinned ? "取消置顶" : "置顶",
            action: () =>
              void sessionUpdateMeta(s().id, { pinned: !s().pinned }).then(props.onChanged),
          },
          { label: "删除会话", danger: true, action: props.onDelete },
        ]);
      }}
      onDblClick={() => {
        setDraft(s().title);
        setRenaming(true);
        setTimeout(() => inputRef?.select(), 0);
      }}
      onDragStart={props.onDragStart}
      onDragOver={props.onDragOver}
      onDrop={props.onDrop}
    >
      <Show when={s().running}>
        <span class="ml-1 w-1.5 h-1.5 rounded-full bg-[var(--ok)] animate-pulse shrink-0" />
      </Show>
      <Show when={s().pinned}>
        <Pin size={10} class="ml-0.5 text-[var(--accent-hover)] shrink-0" />
      </Show>
      <Show
        when={!renaming()}
        fallback={
          <input
            ref={(el) => (inputRef = el)}
            class="flex-1 mx-1 px-1 py-0.5 text-sm bg-transparent border border-[var(--accent)] rounded focus:outline-none"
            value={draft()}
            onInput={(e) => setDraft(e.currentTarget.value)}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              if (e.key === "Enter") void commitRename();
              if (e.key === "Escape") setRenaming(false);
            }}
            onBlur={() => void commitRename()}
          />
        }
      >
        <span class="flex-1 px-2 py-1 truncate" title={s().title}>
          {s().title}
        </span>
      </Show>
      <span class="text-2xs text-[var(--text-faint)] shrink-0 pr-1 group-hover:hidden">
        {relTime(s().updated_at)}
      </span>
      <span class="hidden group-hover:flex items-center shrink-0">
        <button
          class="px-1 text-[var(--text-faint)] hover:text-[var(--text)]"
          title={s().pinned ? "取消置顶" : "置顶"}
          onClick={(e) => void togglePin(e)}
        >
          <Show when={s().pinned} fallback={<Pin size={11} />}>
            <PinOff size={11} />
          </Show>
        </button>
        <Show
          when={!confirming()}
          fallback={
            <>
              <button
                class="px-1 text-[var(--err)]"
                title="确认删除"
                onClick={(e) => {
                  e.stopPropagation();
                  props.onDelete();
                }}
              >
                <Check size={11} />
              </button>
              <button
                class="px-1 text-[var(--text-faint)]"
                title="取消"
                onClick={(e) => {
                  e.stopPropagation();
                  setConfirming(false);
                }}
              >
                <X size={11} />
              </button>
            </>
          }
        >
          <button
            class="px-1 text-[var(--text-faint)] hover:text-[var(--err)]"
            title="删除会话（再点一次确认）"
            onClick={(e) => {
              e.stopPropagation();
              setConfirming(true);
            }}
          >
            <X size={12} />
          </button>
        </Show>
      </span>
    </div>
  );
}
