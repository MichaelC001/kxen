// TextComposer：整卡输入（chips 行 + 自增高 textarea + 内嵌 action bar + 语音 PTT）。
import { createEffect, createSignal, For, Show, onCleanup, onMount } from "solid-js";
import {
  FileText,
  Folder,
  Globe,
  Image as ImageIcon,
  Mic,
  MicOff,
  Plus,
  Send,
  Square,
  X,
} from "lucide-solid";
import { commandList, type CommandInfo, type ContextItem } from "../../lib/chat";
import { buildItems, detectTrigger, type PopupState, type Trigger } from "./triggers";
import { createVoicePtt } from "./voice-ptt";
import AttachMenu from "./AttachMenu";
import MicMenu from "./MicMenu";
import ModelPicker from "./ModelPicker";
import { sendBtn } from "../../lib/variants";

export interface Chip {
  id: string;
  kind: "file" | "dir" | "web" | "docs" | "image" | "knowledge";
  ref: string;
  label: string;
  preview?: string;
}

let chipSeq = 0;

export default function TextComposer(props: {
  streaming: () => boolean;
  onSend: (
    text: string,
    context: ContextItem[],
    images: Array<{ media_type: string; data: string }>,
  ) => void;
  onStop: () => void;
  focusTick: () => number;
}) {
  const [text, setText] = createSignal("");
  const [chips, setChips] = createSignal<Chip[]>([]);
  const [popup, setPopup] = createSignal<(PopupState & Trigger) | null>(null);
  const [commands, setCommands] = createSignal<CommandInfo[]>([]);
  const [recording, setRecording] = createSignal(false);
  const [voiceError, setVoiceError] = createSignal("");
  const [voiceEngine, setVoiceEngine] = createSignal("apple");
  let textareaRef: HTMLTextAreaElement | undefined;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  const estimate = () => Math.ceil(text().length / 4);
  const estimateCls = () =>
    estimate() > 190_000
      ? "text-[var(--err)]"
      : estimate() > 160_000
        ? "text-[var(--warn)]"
        : "text-[var(--text-faint)]";

  onMount(async () => {
    setCommands(await commandList().catch(() => []));
    textareaRef?.focus();
  });
  onCleanup(() => debounceTimer && clearTimeout(debounceTimer));

  createEffect(() => {
    props.focusTick();
    setText("");
    setChips([]);
    setPopup(null);
    textareaRef?.focus();
  });

  const addChip = (chip: Omit<Chip, "id">) => {
    setChips((prev) =>
      prev.some((c) => c.kind === chip.kind && c.ref === chip.ref)
        ? prev
        : [...prev, { ...chip, id: `chip_${chipSeq++}` }],
    );
  };
  const removeChip = (id: string) => setChips((prev) => prev.filter((c) => c.id !== id));

  function attachFiles(files: FileList) {
    for (const file of files) {
      if (file.type.startsWith("image/")) {
        const reader = new FileReader();
        reader.onload = () => {
          const dataUrl = String(reader.result);
          images.set(dataUrl, { media_type: file.type, data: dataUrl.split(",")[1] ?? "" });
          addChip({
            kind: "image",
            ref: dataUrl,
            label: `图片 ${file.type.split("/")[1] ?? ""}`,
            preview: dataUrl,
          });
        };
        reader.readAsDataURL(file);
      } else {
        addChip({ kind: "file", ref: file.name, label: file.name });
      }
    }
  }

  function autoGrow() {
    if (!textareaRef) return;
    textareaRef.style.height = "auto";
    textareaRef.style.height = `${Math.min(textareaRef.scrollHeight, 180)}px`;
  }

  const voiceCtl = createVoicePtt({
    getText: text,
    setText,
    afterChange: autoGrow,
    setRecording,
    setError: setVoiceError,
    engine: voiceEngine,
  });

  function openPopup(trigger: Trigger) {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      const items = await buildItems(trigger, commands(), {
        onChip: (kind, ref, label) => addChip({ kind, ref, label }),
        onPlainInsert: (insert, start) => {
          setPopup(null);
          const cursor = textareaRef?.selectionStart ?? text().length;
          setText((prev) => prev.slice(0, start) + insert + prev.slice(cursor));
          textareaRef?.focus();
        },
        onCloseToken: (start) => {
          const cursor = textareaRef?.selectionStart ?? text().length;
          setText((prev) => prev.slice(0, start) + prev.slice(cursor));
          setPopup(null);
          textareaRef?.focus();
        },
      });
      setPopup({ ...trigger, items, selected: 0 });
    }, 200);
  }

  function onInput(e: InputEvent & { currentTarget: HTMLTextAreaElement }) {
    const value = e.currentTarget.value;
    setText(value);
    autoGrow();
    const cursor = e.currentTarget.selectionStart ?? value.length;
    const trigger = detectTrigger(value, cursor);
    if (trigger) {
      openPopup(trigger);
    } else {
      setPopup(null);
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    const p = popup();
    if (p) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const delta = e.key === "ArrowDown" ? 1 : -1;
        setPopup({ ...p, selected: (p.selected + delta + p.items.length) % p.items.length });
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        p.items[p.selected]?.apply();
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        setPopup(null);
        return;
      }
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send();
      return;
    }
    if (e.key === "Backspace" && text() === "" && chips().length > 0) {
      removeChip(chips().at(-1)!.id);
      return;
    }
    voiceCtl.onSpaceDown(e);
  }

  function onKeyUp(e: KeyboardEvent) {
    voiceCtl.onSpaceUp(e);
  }

  function onPaste(e: ClipboardEvent) {
    for (const item of e.clipboardData?.items ?? []) {
      if (!item.type.startsWith("image/")) continue;
      e.preventDefault();
      const file = item.getAsFile();
      if (!file) continue;
      const reader = new FileReader();
      reader.onload = () => {
        const dataUrl = String(reader.result);
        images.set(dataUrl, { media_type: file.type, data: dataUrl.split(",")[1] ?? "" });
        addChip({
          kind: "image",
          ref: dataUrl,
          label: `图片 ${file.type.split("/")[1] ?? ""}`,
          preview: dataUrl,
        });
      };
      reader.readAsDataURL(file);
    }
  }
  const images = new Map<string, { media_type: string; data: string }>();

  async function send() {
    const value = text().trim();
    if ((!value && chips().length === 0) || props.streaming()) return;
    const context: ContextItem[] = chips()
      .filter((c) => c.kind !== "image")
      .map((c) => {
        if (c.kind === "dir") return { type: "dir", path: c.ref };
        if (c.kind === "web" || c.kind === "docs") return { type: c.kind, url: c.ref };
        return { type: "file", path: c.ref };
      });
    const imageParts = chips()
      .filter((c) => c.kind === "image")
      .map((c) => images.get(c.ref))
      .filter((i): i is { media_type: string; data: string } => !!i);
    const knowledgeNote = chips()
      .filter((c) => c.kind === "knowledge")
      .map(
        (c) => `（请把本次相关经验沉淀到 ${c.ref}，frontmatter 带 type/description，写前给我确认）`,
      )
      .join("\n");
    if (recording()) voiceCtl.stop();
    props.onSend(knowledgeNote ? `${value}\n${knowledgeNote}` : value, context, imageParts);
    setText("");
    setChips([]);
    autoGrow();
  }

  return (
    <div class="relative">
      <Show when={popup() && popup()!.items.length > 0}>
        <div class="composer-popup absolute bottom-full left-0 right-0 mb-1.5 rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] shadow-xl shadow-black/30 overflow-hidden z-20">
          <For each={popup()!.items}>
            {(item, i) => (
              <button
                class="popup-row"
                classList={{ "popup-row-active": i() === popup()!.selected }}
                onMouseDown={(e) => {
                  e.preventDefault();
                  item.apply();
                }}
              >
                <span class="truncate flex-1 font-mono">{item.label}</span>
                <Show when={item.detail}>
                  <span class="text-2xs text-[var(--text-faint)] popup-detail truncate">
                    {item.detail}
                  </span>
                </Show>
                <Show when={item.badge}>
                  <span class="text-2xs px-1 rounded border border-[var(--border)] text-[var(--text-faint)]">
                    {item.badge}
                  </span>
                </Show>
              </button>
            )}
          </For>
        </div>
      </Show>

      <div class="composer-card" classList={{ recording: recording() }}>
        <Show when={chips().length > 0}>
          <div class="flex flex-wrap gap-1.5 px-2 pt-2">
            <For each={chips()}>
              {(chip) => (
                <span class="chip-token">
                  {chip.kind === "file" && <FileText size={11} />}
                  {chip.kind === "dir" && <Folder size={11} />}
                  {(chip.kind === "web" || chip.kind === "docs") && <Globe size={11} />}
                  {chip.kind === "image" &&
                    (chip.preview ? (
                      <img src={chip.preview} class="chip-img" alt="" />
                    ) : (
                      <ImageIcon size={11} />
                    ))}
                  {chip.kind === "knowledge" && <Plus size={11} />}
                  <span class="chip-label">{chip.label}</span>
                  <button class="chip-x" onClick={() => removeChip(chip.id)}>
                    <X size={11} />
                  </button>
                </span>
              )}
            </For>
          </div>
        </Show>

        <textarea
          ref={(el) => (textareaRef = el)}
          class="composer-textarea"
          rows={2}
          placeholder={
            recording()
              ? "语音输入中…松开空格完成"
              : "输入消息，@ 引用文件，/ 命令，# 沉淀知识，长按空格语音"
          }
          value={text()}
          onInput={onInput}
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
            classList={{
              "send-ready": !props.streaming() && (text().trim() !== "" || chips().length > 0),
            }}
            onClick={() => (props.streaming() ? props.onStop() : void send())}
            disabled={!props.streaming() && text().trim() === "" && chips().length === 0}
            title={props.streaming() ? "停止" : "发送"}
          >
            {props.streaming() ? <Square size={13} /> : <Send size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}
