// Lexical Composer：mention 内联 + 触发弹窗 + 语音 PTT + 整卡 action bar。
import { createEffect, createSignal, Show, onCleanup, onMount } from "solid-js";
import { Mic, MicOff, Plus, Send, Square } from "lucide-solid";
import { $getRoot, COMMAND_PRIORITY_HIGH, KEY_ENTER_COMMAND, type LexicalEditor } from "lexical";
import { commandList, fsComplete, type CommandInfo, type ContextItem } from "../../lib/chat";
import { speechSupported } from "../../lib/voice";
import { clearEditor, extractPayload, replaceTriggerWithMention, setupEditor } from "./editor";
import { detectTrigger, type Trigger } from "./triggers";
import ComposerPopup, { usePopupSelection, type PopupItem } from "./Popup";
import ModelPill from "./ModelPill";
import { createVoiceController } from "./voice";
import { createPasteHandler, type ImagePart } from "./paste";
import type { MentionData } from "./MentionNode";
import { $getSelection, $isRangeSelection } from "lexical";

const KNOWLEDGE_TARGETS = [
  { ref: ".agents/rules/", label: "写入项目规范", detail: ".agents/rules/（入 git 共享）" },
  { ref: "~/.agents/rules/", label: "写入全局规范", detail: "~/.agents/rules/（个人全部项目）" },
  { ref: ".kxen/memory/", label: "写入本地 memory", detail: ".kxen/memory/（本机，gitignored）" },
];

export default function LexicalComposer(props: {
  streaming: () => boolean;
  onSend: (text: string, context: ContextItem[], images: ImagePart[]) => void;
  onStop: () => void;
  focusTick: () => number;
}) {
  const [commands, setCommands] = createSignal<CommandInfo[]>([]);
  const [recording, setRecording] = createSignal(false);
  const [voiceError, setVoiceError] = createSignal("");
  const [textLength, setTextLength] = createSignal(0);
  let editor: LexicalEditor | null = null;
  let containerRef: HTMLDivElement | undefined;
  let editorRef: HTMLDivElement | undefined;
  let currentTrigger: Trigger | null = null;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  const popupCtl = usePopupSelection();
  const voice = createVoiceController(() => editor, setRecording, setVoiceError);
  const imageStore = new Map<string, ImagePart>();
  const onPaste = createPasteHandler(() => editor, imageStore);

  const estimate = () => Math.ceil(textLength() / 4);
  const estimateCls = () =>
    estimate() > 190_000
      ? "text-[var(--err)]"
      : estimate() > 160_000
        ? "text-[var(--warn)]"
        : "text-[var(--text-faint)]";

  onMount(async () => {
    if (!editorRef) return;
    editor = setupEditor(editorRef);
    editor.registerUpdateListener(() => {
      if (!editor) return;
      const plain = editor.read(() => $getRoot().getTextContent());
      setTextLength(plain.length);
      const trigger = detectTrigger(editor);
      currentTrigger = trigger;
      if (trigger) {
        void loadItems(trigger);
      } else {
        popupCtl.close();
      }
    });
    editor.registerCommand(
      KEY_ENTER_COMMAND,
      (event: KeyboardEvent) => {
        if (popupCtl.popup()) {
          popupCtl.applySelected();
          return true;
        }
        if (!event.shiftKey) {
          void send();
          return true;
        }
        return false;
      },
      COMMAND_PRIORITY_HIGH,
    );
    editorRef.focus();
    setCommands(await commandList().catch(() => []));
  });
  onCleanup(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
    voice.stop();
    editor?.setRootElement(null);
  });

  // focusTick 变化 = 切会话：清空编辑器
  createEffect(() => {
    props.focusTick();
    if (editor) clearEditor(editor);
  });

  async function loadItems(trigger: Trigger) {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      if (!containerRef) return;
      const rect = trigger.rect;
      const container = containerRef.getBoundingClientRect();
      if (trigger.kind === "at") {
        const hits = await fsComplete(trigger.query, 10).catch(() => []);
        popupCtl.open(
          hits.map((h) => ({
            label: h.path,
            badge: h.kind === "dir" ? "dir" : undefined,
            section: "文件",
            apply: () =>
              applyMention({
                kind: h.kind === "dir" ? "dir" : "file",
                ref: h.path,
                label: h.path.split("/").pop() ?? h.path,
              }),
          })),
          rect,
          container,
        );
      } else if (trigger.kind === "slash") {
        const q = trigger.query.toLowerCase();
        const matched = commands().filter((c) => c.name.toLowerCase().includes(q));
        const toItem = (c: CommandInfo, section: string): PopupItem => ({
          label: `/${c.name}${c.argument_hint ? ` ${c.argument_hint}` : ""}`,
          detail: c.description,
          badge: c.kind === "skill" ? "skill" : undefined,
          section,
          apply: () => insertPlain(`/${c.name} `),
        });
        const cmds = matched
          .filter((c) => c.kind !== "skill")
          .slice(0, 6)
          .map((c) => toItem(c, "命令"));
        const skills = matched
          .filter((c) => c.kind === "skill")
          .slice(0, 6)
          .map((c) => toItem(c, "skills"));
        popupCtl.open([...cmds, ...skills], rect, container);
      } else {
        const q = trigger.query.toLowerCase();
        popupCtl.open(
          KNOWLEDGE_TARGETS.filter((k) => k.label.toLowerCase().includes(q)).map((k) => ({
            label: k.label,
            detail: k.detail,
            badge: "knowledge",
            section: "知识沉淀",
            apply: () => applyMention({ kind: "knowledge", ref: k.ref, label: k.label }),
          })),
          rect,
          container,
        );
      }
    }, 200);
  }

  function applyMention(mention: MentionData) {
    if (!editor || !currentTrigger) return;
    replaceTriggerWithMention(
      editor,
      currentTrigger.startInNode,
      currentTrigger.caretOffset,
      mention,
    );
    popupCtl.close();
    editorRef?.focus();
  }

  function insertPlain(text: string) {
    if (!editor || !currentTrigger) return;
    // /命令保留为纯文本（连参数发送）
    editor.update(() => {
      const selection = $getSelection();
      if (!$isRangeSelection(selection)) return;
      const anchor = selection.anchor;
      const node = anchor.getNode();
      if ("setTextContent" in node) {
        const content = (node as { getTextContent: () => string }).getTextContent();
        const before = content.slice(0, currentTrigger!.startInNode);
        (node as { setTextContent: (t: string) => void }).setTextContent(
          before + text + content.slice(currentTrigger!.caretOffset),
        );
        (node as { select: (a: number, b: number) => void }).select(
          before.length + text.length,
          before.length + text.length,
        );
      }
    });
    popupCtl.close();
    editorRef?.focus();
  }

  function onEditorKeyDown(e: KeyboardEvent) {
    const p = popupCtl.popup();
    if (p) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        popupCtl.move(e.key === "ArrowDown" ? 1 : -1);
        return;
      }
      if (e.key === "Tab") {
        e.preventDefault();
        popupCtl.applySelected();
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        popupCtl.close();
        return;
      }
    }
    voice.onSpaceDown(e);
  }

  async function send() {
    if (!editor || props.streaming()) return;
    const payload = extractPayload(editor);
    if (!payload.text.trim() && payload.mentions.length === 0) return;
    const context: ContextItem[] = payload.mentions
      .filter((m) => m.kind !== "image")
      .map((m) => {
        if (m.kind === "dir") return { type: "dir", path: m.ref };
        if (m.kind === "web" || m.kind === "docs") return { type: m.kind, url: m.ref };
        return { type: "file", path: m.ref };
      });
    const images = payload.mentions
      .filter((m) => m.kind === "image")
      .map((m) => imageStore.get(m.ref))
      .filter((i): i is ImagePart => !!i);
    const knowledgeNote = payload.mentions
      .filter((m) => m.kind === "knowledge")
      .map(
        (m) => `（请把本次相关经验沉淀到 ${m.ref}，frontmatter 带 type/description，写前给我确认）`,
      )
      .join("\n");
    if (recording()) voice.stop();
    props.onSend(
      knowledgeNote ? `${payload.text}\n${knowledgeNote}` : payload.text,
      context,
      images,
    );
    clearEditor(editor);
  }

  return (
    <div ref={(el) => (containerRef = el)} class="relative">
      <Show when={popupCtl.popup()}>{(p) => <ComposerPopup popup={p()} />}</Show>

      <div class="composer-card" classList={{ recording: recording() }}>
        <div
          ref={(el) => (editorRef = el)}
          class="editor-root focus:outline-none"
          onKeyDown={onEditorKeyDown}
          onKeyUp={voice.onSpaceUp}
          onPaste={onPaste}
        />

        <div class="composer-actionbar">
          <button
            class="pressable action-icon"
            title="附件（@ 文件 / 粘贴图片）"
            onClick={() => editorRef?.focus()}
          >
            <Plus size={15} />
          </button>
          <button
            class="pressable action-icon"
            classList={{ "text-[var(--err)] animate-pulse": recording() }}
            title={recording() ? "停止语音输入" : "语音输入（长按空格或点击，Apple 本地识别）"}
            onClick={() => {
              if (!speechSupported()) {
                setVoiceError("当前环境不支持语音识别");
                return;
              }
              if (recording()) voice.stop();
              else voice.start();
            }}
          >
            {recording() ? <MicOff size={15} /> : <Mic size={15} />}
          </button>
          <Show when={voiceError()}>
            <span class="text-2xs text-[var(--err)]">{voiceError()}</span>
          </Show>
          <span class={`text-2xs tabular-nums ml-auto ${estimateCls()}`}>~{estimate()} tok</span>
          <ModelPill />
          <button
            class="pressable send-btn"
            classList={{ "send-btn-stop": props.streaming() }}
            onClick={() => (props.streaming() ? props.onStop() : void send())}
            disabled={!props.streaming() && textLength() === 0}
            title={props.streaming() ? "停止" : "发送"}
          >
            {props.streaming() ? <Square size={13} /> : <Send size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}
