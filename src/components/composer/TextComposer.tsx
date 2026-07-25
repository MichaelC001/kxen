// TextComposer：Cline 式 textarea 整卡输入（IME/undo/选区全原生免疫）。
// @//# 触发弹层（任意位置，光标前切片判定）+ 框外 row chip + 大粘贴折叠占位 + 语音 PTT + 每会话草稿。
import { createEffect, createSignal, Show, onCleanup, onMount } from "solid-js";
import { Send, Square } from "lucide-solid";
import { commandList, type CommandInfo, type ContextItem } from "../../lib/chat";
import { activeSessionId, ensureActiveSession } from "../../lib/state";
import { clearDraft, getDraft, setDraft } from "../../lib/drafts";
import { COMPOSER_INSERT_EVENT } from "../../lib/composer-bus";
import { buildItems, detectTrigger, type PopupState, type Trigger } from "./triggers";
import { fsResolveName, resolveAttachPath, resolvePickedPath } from "./attach";
import { createVoicePtt } from "./voice-ptt";
import { caretRect } from "./caret";
import { createPasteStore, isLargePaste, normalizePaste } from "./paste";
import AttachMenu from "./AttachMenu";
import ComposerPopup from "./ComposerPopup";
import MicControl from "./MicControl";
import ModelPicker from "./ModelPicker";
import RowChips, { type RowChip } from "./RowChips";
import { sendBtn } from "../../lib/variants";

let chipSeq = 0;
const MAX_HEIGHT = 176; // styles 里 max-h-44 同值

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
  const [popup, setPopup] = createSignal<(PopupState & Trigger) | null>(null);
  const [popupPos, setPopupPos] = createSignal<{ left: number; bottom: number } | null>(null);
  const [commands, setCommands] = createSignal<CommandInfo[]>([]);
  const [rowChips, setRowChips] = createSignal<RowChip[]>([]);
  const [recording, setRecording] = createSignal(false),
    [activeVoice, setActiveVoice] = createSignal("");
  const [voiceError, setVoiceError] = createSignal(""),
    [voiceEngine, setVoiceEngine] = createSignal("apple");
  let ta: HTMLTextAreaElement | undefined;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  let imeLockUntil = 0; // Safari compositionend 先于 commit keydown（WebKit #165231），50ms 锁窗吞尾随 Enter
  const images = new Map<string, { media_type: string; data: string }>();
  const pastes = createPasteStore();

  const estimate = () => Math.ceil(text().length / 4);
  const estimateCls = () =>
    estimate() > 190_000
      ? "text-[var(--err)]"
      : estimate() > 160_000
        ? "text-[var(--warn)]"
        : "text-[var(--text-faint)]";

  function autogrow() {
    if (!ta) return;
    ta.style.height = "auto";
    ta.style.height = `${Math.min(ta.scrollHeight, MAX_HEIGHT)}px`;
  }

  function setValue(v: string, caret?: number) {
    if (!ta) return;
    ta.value = v;
    setText(v);
    const pos = caret ?? v.length;
    ta.setSelectionRange(pos, pos);
    autogrow();
  }

  function insertAtCaret(insert: string) {
    if (!ta) return;
    const pos = ta.selectionStart;
    ta.setRangeText(insert, pos, ta.selectionEnd, "end");
    setText(ta.value);
    autogrow();
  }

  /** 删除触发词文本（@xxx / /xxx / #xxx 段），光标归位到删除点。 */
  function removeTriggerText(trigger: Trigger, from?: number) {
    const start = from ?? trigger.start;
    // 定界扫触发段（触发符到下个空白）而非光标位置：光标可能已移出触发段，按光标 slice 会重复中段
    const t = text();
    let end = trigger.start + 1;
    while (end < t.length && !" \t\n　".includes(t[end]!)) end++;
    setValue(t.slice(0, start) + t.slice(end), start);
  }

  /** 光标移出触发段（触发符到查询词尾）即关弹层：click/方向键/Home/End 等不走 input 的位移。 */
  function closePopupIfCaretOut() {
    const p = popup();
    if (!p || !ta) return;
    const pos = ta.selectionStart;
    if (pos <= p.start || pos > p.start + 1 + p.query.length) setPopup(null);
  }

  onMount(() => {
    void commandList()
      .then(setCommands)
      .catch(() => setCommands([]));
    const onInsert = (e: Event) => {
      insertAtCaret((e as CustomEvent<string>).detail);
      ta?.focus();
    };
    window.addEventListener(COMPOSER_INSERT_EVENT, onInsert);
    onCleanup(() => window.removeEventListener(COMPOSER_INSERT_EVENT, onInsert));
    ta?.focus();
  });
  onCleanup(() => debounceTimer && clearTimeout(debounceTimer));

  createEffect(() => {
    props.focusTick();
    // 每会话草稿：切走前已持续落盘，切回恢复；row chip 不跨会话保留
    const d = getDraft(activeSessionId());
    setRowChips([]);
    setPopup(null);
    setValue(d);
    ta?.focus();
  });

  const voiceCtl = createVoicePtt({
    getText: () => text(),
    setText: (v) => setValue(v),
    afterChange: () => {},
    setRecording,
    setError: setVoiceError,
    engine: voiceEngine,
    sessionId: () => activeSessionId(),
    onStarted: setActiveVoice,
  });

  function checkTrigger() {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      const cursor = ta?.selectionStart ?? text().length;
      const trigger = detectTrigger(text(), cursor);
      if (!trigger) {
        setPopup(null);
        return;
      }
      const items = await buildItems(trigger, commands(), {
        onChip: (kind, ref, label) => {
          removeTriggerText(trigger);
          pushChip({ kind, ref, label, title: ref });
          setPopup(null);
          ta?.focus();
        },
        onPlainInsert: (insert, start) => {
          removeTriggerText(trigger, start);
          insertAtCaret(insert);
          setPopup(null);
          ta?.focus();
        },
        onCloseToken: () => {
          removeTriggerText(trigger);
          setPopup(null);
          ta?.focus();
        },
      });
      const rect = ta ? caretRect(ta) : null;
      if (items.length === 0) {
        setPopup(null);
        return;
      }
      // composer 贴窗口底部，弹窗必须向上展开（bottom 锚定），否则下穿出窗被状态栏裁掉
      const pos = rect && {
        left: Math.max(8, Math.min(rect.left, window.innerWidth - 264)),
        bottom: window.innerHeight - rect.top + 4,
      };
      setPopupPos(pos || null);
      setPopup({ ...trigger, items, selected: 0 });
    }, 200);
  }

  const pushChip = (chip: Omit<RowChip, "id">) =>
    setRowChips((prev) => [...prev, { id: `chip_${chipSeq++}`, ...chip }]);

  function attachFiles(files: FileList | File[]) {
    for (const file of files) {
      if (file.type.startsWith("image/")) {
        const reader = new FileReader();
        reader.onload = () => {
          const dataUrl = String(reader.result);
          images.set(dataUrl, { media_type: file.type, data: dataUrl.split(",")[1] ?? "" });
          pushChip({
            kind: "image",
            ref: dataUrl,
            label: `图片 ${file.type.split("/")[1] ?? ""}`,
            preview: dataUrl,
          });
        };
        reader.readAsDataURL(file);
      } else {
        void attachOneFile(file);
      }
    }
  }

  /** 普通文件附件：File 只有 basename，反查 workspace 索引存相对路径（子目录可读、同名不串）。 */
  async function attachOneFile(file: File) {
    const candidates = await fsResolveName(file.name).catch(() => []);
    const rel = resolveAttachPath(file.name, file.size, candidates) ?? file.name;
    pushChip({ kind: "file", ref: rel, label: file.name, title: rel });
  }

  /** 原生对话框附件：真实绝对路径。授权绑会话（草稿态先落库）；图片读 base64 内联，文件走 context chip。 */
  async function attachPaths(paths: string[]) {
    const sid = await ensureActiveSession();
    for (const path of paths) {
      const chip = await resolvePickedPath(sid, path);
      if (!chip) continue;
      if (chip.kind === "image") {
        images.set(chip.ref, chip.image);
        pushChip({
          kind: "image",
          ref: chip.ref,
          label: chip.label,
          title: chip.title,
          preview: chip.ref,
        });
      } else {
        pushChip({ kind: "file", ref: chip.ref, label: chip.label, title: chip.title });
      }
    }
  }

  function onPaste(e: ClipboardEvent) {
    const files = e.clipboardData?.files;
    if (files && files.length > 0) {
      e.preventDefault();
      attachFiles(files);
      return;
    }
    const raw = e.clipboardData?.getData("text/plain") ?? "";
    const text = normalizePaste(raw);
    if (isLargePaste(text)) {
      e.preventDefault();
      insertAtCaret(pastes.add(text));
    }
    // 小粘贴走原生（textarea 默认行为全对）
  }

  function onKeyDown(e: KeyboardEvent) {
    const p = popup();
    if (p) {
      // IME 组字中弹层放行：Enter/方向键归输入法候选窗（isComposing/keyCode229/锁窗三保险，同发送守卫）
      if (e.isComposing || e.keyCode === 229 || Date.now() < imeLockUntil) return;
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
    // IME 提交 Enter 不发送：isComposing / keyCode 229 / 50ms 锁窗 三保险（cline#3475 同款）
    if (
      e.key === "Enter" &&
      !e.shiftKey &&
      !e.isComposing &&
      e.keyCode !== 229 &&
      Date.now() >= imeLockUntil
    ) {
      e.preventDefault();
      void send();
      return;
    }
    voiceCtl.onSpaceDown(e);
  }

  async function send() {
    const value = pastes.expand(text()).trim();
    if (!value && rowChips().length === 0) return;
    // 知识注记走 note context（注入模型但不进用户气泡，Part::Context 分流）
    const context: ContextItem[] = rowChips()
      .filter((c) => c.kind !== "image")
      .map((c) =>
        c.kind === "knowledge"
          ? {
              type: "note",
              text: `（请把本次相关经验用 knowledge 工具沉淀到 ${c.ref}，写前给我确认）`,
            }
          : c.kind === "web" || c.kind === "docs"
            ? { type: c.kind, url: c.ref }
            : c.kind === "dir"
              ? { type: "dir", path: c.ref }
              : { type: "file", path: c.ref },
      );
    const imageParts = rowChips()
      .filter((c) => c.kind === "image")
      .map((c) => images.get(c.ref))
      .filter((i): i is { media_type: string; data: string } => !!i);
    if (recording()) voiceCtl.stop();
    props.onSend(value, context, imageParts);
    clearDraft(activeSessionId());
    pastes.clear();
    setValue("", 0);
    setRowChips([]);
  }

  return (
    <div class="relative">
      <Show when={popup()}>
        {(p) => <ComposerPopup items={p().items} selected={p().selected} pos={popupPos()} />}
      </Show>
      <div class="composer-card rounded-xl relative" classList={{ recording: recording() }}>
        <RowChips
          chips={rowChips()}
          onRemove={(id) => setRowChips((prev) => prev.filter((c) => c.id !== id))}
        />
        <textarea
          ref={(el) => (ta = el)}
          rows={1}
          class="w-full resize-none bg-transparent px-3 py-2.5 text-sm outline-none placeholder:text-[var(--text-faint)]"
          style="overflow-y: auto;"
          placeholder="输入消息，@ 引用 · / 命令 · # 知识 · 长按空格语音"
          onInput={() => {
            if (ta) setText(ta.value);
            setDraft(activeSessionId(), ta?.value ?? "");
            autogrow();
            checkTrigger();
          }}
          onKeyDown={onKeyDown}
          onKeyUp={(e) => {
            voiceCtl.onSpaceUp(e);
            closePopupIfCaretOut();
          }}
          onClick={closePopupIfCaretOut}
          onBlur={() => setPopup(null)}
          onPaste={onPaste}
          onCompositionEnd={() => (imeLockUntil = Date.now() + 50)}
        />
        <div class="composer-actionbar">
          <AttachMenu onPaths={(paths) => void attachPaths(paths)} />
          <MicControl
            recording={recording}
            activeVoice={activeVoice}
            voiceError={voiceError}
            onToggle={() => voiceCtl.toggle()}
            onEngine={setVoiceEngine}
          />
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
