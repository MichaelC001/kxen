// LexicalComposer：Lexical 内核整卡输入（inline token chips + 弹窗锚定 caret + 语音 PTT + 图片粘贴）。
import { createEffect, createSignal, For, Show, onCleanup, onMount } from "solid-js";
import { Image as ImageIcon, Mic, MicOff, Plus, Send, Square, X } from "lucide-solid";
import { commandList, type CommandInfo, type ContextItem } from "../../lib/chat";
import { buildItems, detectTrigger, type PopupState, type Trigger } from "./triggers";
import { mountComposer, type ChipData, type ComposerCore } from "./lexical-core";
import { createVoicePtt } from "./voice-ptt";
import AttachMenu from "./AttachMenu";
import MicMenu from "./MicMenu";
import ModelPicker from "./ModelPicker";
import { sendBtn } from "../../lib/variants";

interface RowChip {
  id: string;
  kind: "image" | "knowledge";
  ref: string;
  label: string;
  preview?: string;
}

let chipSeq = 0;

export default function LexicalComposer(props: {
  streaming: () => boolean;
  onSend: (
    text: string,
    context: ContextItem[],
    images: Array<{ media_type: string; data: string }>,
  ) => void;
  onStop: () => void;
  focusTick: () => number;
}) {
  const [popup, setPopup] = createSignal<(PopupState & Trigger) | null>(null);
  const [popupPos, setPopupPos] = createSignal<{ left: number; top: number } | null>(null);
  const [commands, setCommands] = createSignal<CommandInfo[]>([]);
  const [rowChips, setRowChips] = createSignal<RowChip[]>([]);
  const [recording, setRecording] = createSignal(false);
  const [voiceError, setVoiceError] = createSignal("");
  const [voiceEngine, setVoiceEngine] = createSignal("apple");
  const [tick, setTick] = createSignal(0); // 文本变化驱动（estimate / 触发检测）
  let rootEl: HTMLDivElement | undefined;
  let core: ComposerCore | undefined;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  const images = new Map<string, { media_type: string; data: string }>();

  const estimate = () => {
    void tick(); // 依赖文本变化信号
    return Math.ceil((core?.getText().length ?? 0) / 4);
  };
  const estimateCls = () =>
    estimate() > 190_000
      ? "text-[var(--err)]"
      : estimate() > 160_000
        ? "text-[var(--warn)]"
        : "text-[var(--text-faint)]";
  const empty = () => {
    void tick(); // 依赖文本变化信号
    return (core?.getText() ?? "") === "" && rowChips().length === 0;
  };

  onMount(async () => {
    setCommands(await commandList().catch(() => []));
    if (rootEl) {
      core = mountComposer(rootEl);
      core.setText(""); // WebKit 初始渲染（空 contenteditable 无 caret）
      core.onTextChange(() => {
        setTick((t) => t + 1);
        checkTrigger();
      });
      core.focus();
    }
  });
  onCleanup(() => debounceTimer && clearTimeout(debounceTimer));

  createEffect(() => {
    props.focusTick();
    core?.clear();
    setRowChips([]);
    setPopup(null);
    core?.focus();
  });

  const voiceCtl = createVoicePtt({
    getText: () => core?.getText() ?? "",
    setText: (v) => core?.setText(v),
    afterChange: () => setTick((t) => t + 1),
    setRecording,
    setError: setVoiceError,
    engine: voiceEngine,
  });

  function checkTrigger() {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      const text = core?.getText() ?? "";
      const offset = core?.caretOffset() ?? text.length;
      const trigger = detectTrigger(text, offset);
      if (!trigger) {
        setPopup(null);
        return;
      }
      const items = await buildItems(trigger, commands(), {
        onChip: (kind, ref, label) => {
          removeTriggerText(trigger);
          if (kind === "knowledge") {
            setRowChips((prev) => [
              ...prev,
              { id: `chip_${chipSeq++}`, kind: "knowledge", ref, label },
            ]);
          } else {
            core?.insertChip({ kind, ref, label } as ChipData);
          }
          setPopup(null);
          core?.focus();
        },
        onPlainInsert: (insert, start) => {
          removeTriggerText(trigger, start);
          core?.insertPlain(insert);
          setPopup(null);
          core?.focus();
        },
        onCloseToken: () => {
          removeTriggerText(trigger);
          setPopup(null);
          core?.focus();
        },
      });
      const rect = core?.caretRect();
      setPopupPos(rect ? { left: rect.left, top: rect.bottom + 4 } : null);
      setPopup({ ...trigger, items, selected: 0 });
    }, 200);
  }

  /** 删除触发词文本（@xxx / /xxx / #xxx 段），光标归位到删除点。 */
  function removeTriggerText(trigger: Trigger, from?: number) {
    const text = core?.getText() ?? "";
    const offset = core?.caretOffset() ?? text.length;
    const start = from ?? trigger.start;
    core?.setText(text.slice(0, start) + text.slice(offset));
    core?.setCaret(start);
  }

  function attachFiles(files: FileList) {
    for (const file of files) {
      if (file.type.startsWith("image/")) {
        const reader = new FileReader();
        reader.onload = () => {
          const dataUrl = String(reader.result);
          images.set(dataUrl, { media_type: file.type, data: dataUrl.split(",")[1] ?? "" });
          setRowChips((prev) => [
            ...prev,
            {
              id: `chip_${chipSeq++}`,
              kind: "image",
              ref: dataUrl,
              label: `图片 ${file.type.split("/")[1] ?? ""}`,
              preview: dataUrl,
            },
          ]);
        };
        reader.readAsDataURL(file);
      } else {
        core?.insertChip({ kind: "file", ref: file.name, label: file.name });
      }
    }
  }

  function onPaste(e: ClipboardEvent) {
    const files = e.clipboardData?.files;
    if (files && files.length > 0) {
      e.preventDefault();
      attachFiles(files);
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    const p = popup();
    if (p) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        const delta = e.key === "ArrowDown" ? 1 : -1;
        setPopup({ ...p, selected: (p.selected + delta + p.items.length) % p.items.length });
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        e.stopPropagation();
        p.items[p.selected]?.apply();
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        setPopup(null);
        return;
      }
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      void send();
      return;
    }
    voiceCtl.onSpaceDown(e);
  }

  function onKeyUp(e: KeyboardEvent) {
    voiceCtl.onSpaceUp(e);
  }

  async function send() {
    if (!core) return;
    const value = core.getText().trim();
    const inlineChips = core.extractChips();
    const knowledgeNote = rowChips()
      .filter((c) => c.kind === "knowledge")
      .map((c) => `（请把本次相关经验用 knowledge 工具沉淀到 ${c.ref}，写前给我确认）`)
      .join("\n");
    if (!value && inlineChips.length === 0 && rowChips().length === 0) return;
    const context: ContextItem[] = inlineChips.map((c) => {
      if (c.kind === "web" || c.kind === "docs") return { type: c.kind, url: c.ref };
      return { type: "file", path: c.ref };
    });
    const imageParts = rowChips()
      .filter((c) => c.kind === "image")
      .map((c) => images.get(c.ref))
      .filter((i): i is { media_type: string; data: string } => !!i);
    if (recording()) voiceCtl.stop();
    props.onSend(knowledgeNote ? `${value}\n${knowledgeNote}` : value, context, imageParts);
    core.clear();
    setRowChips([]);
    setTick((t) => t + 1);
  }

  return (
    <div class="relative">
      <Show when={popup()}>
        {(p) => (
          <div
            class="composer-popup fixed w-64 max-h-72 overflow-auto rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] shadow-xl shadow-black/30 z-30"
            style={
              popupPos()
                ? `left:${popupPos()!.left}px;top:${popupPos()!.top}px`
                : "left:16px;bottom:120px"
            }
          >
            <For each={p().items}>
              {(item, i) => (
                <button
                  class="popup-row w-full"
                  classList={{ "bg-[var(--bg-overlay)]": i() === p().selected }}
                  onClick={() => item.apply()}
                >
                  <span class="flex-1 text-left truncate">{item.label}</span>
                  <Show when={item.detail}>
                    <span class="text-2xs text-[var(--text-faint)] truncate">{item.detail}</span>
                  </Show>
                </button>
              )}
            </For>
          </div>
        )}
      </Show>
      <div class="composer-card rounded-xl relative" classList={{ recording: recording() }}>
        <Show when={empty()}>
          <div class="editor-placeholder">
            {recording()
              ? "语音输入中…松开空格完成"
              : "输入消息，@ 引用文件，/ 命令，# 沉淀知识，长按空格语音"}
          </div>
        </Show>
        <Show when={rowChips().length > 0}>
          <div class="flex flex-wrap gap-1.5 px-3 pt-2.5">
            <For each={rowChips()}>
              {(chip) => (
                <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded border border-[var(--border)] bg-[var(--bg-overlay)] text-2xs">
                  {chip.kind === "image" ? (
                    chip.preview ? (
                      <img src={chip.preview} alt="" class="w-4 h-4 rounded object-cover" />
                    ) : (
                      <ImageIcon size={11} />
                    )
                  ) : (
                    <Plus size={11} />
                  )}
                  <span class="max-w-32 truncate">{chip.label}</span>
                  <button
                    class="text-[var(--text-faint)] hover:text-[var(--err)]"
                    onClick={() => setRowChips((prev) => prev.filter((c) => c.id !== chip.id))}
                  >
                    <X size={11} />
                  </button>
                </span>
              )}
            </For>
          </div>
        </Show>
        <div
          ref={(el) => (rootEl = el)}
          class="editor-root px-3 py-2.5 min-h-14 max-h-44 overflow-y-auto text-sm outline-none wrap-break-word whitespace-pre-wrap"
          onKeyDown={onKeyDown}
          onKeyUp={onKeyUp}
          onPaste={onPaste}
        />
        <div class="composer-actionbar">
          <AttachMenu onFiles={attachFiles} />
          <div class="relative flex items-center">
            <button
              class="pressable action-icon mic-btn"
              classList={{ "mic-recording": recording() }}
              title={recording() ? "停止语音输入" : "语音输入（长按空格或点击）"}
              onClick={() => voiceCtl.toggle()}
            >
              {recording() ? <MicOff size={15} /> : <Mic size={15} />}
            </button>
            <MicMenu onEngine={setVoiceEngine} />
          </div>
          <Show when={voiceError()}>
            <span class="text-2xs text-[var(--err)]">{voiceError()}</span>
          </Show>
          <span class={`text-2xs tabular-nums ml-auto ${estimateCls()}`}>~{estimate()} tok</span>
          <ModelPicker />
          <button
            class={sendBtn({ intent: props.streaming() ? "danger" : "primary" })}
            classList={{ "send-ready": !props.streaming() }}
            onClick={() => (props.streaming() ? props.onStop() : void send())}
            title={props.streaming() ? "停止" : "发送"}
          >
            {props.streaming() ? <Square size={13} /> : <Send size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}
