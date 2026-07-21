import { createEffect, createSignal, For, Show, onCleanup, onMount } from "solid-js";
import {
  ChevronDown,
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
import { speechSupported, startVoice, type VoiceSession } from "../lib/voice";
import {
  commandList,
  currentModel,
  fsComplete,
  setModel,
  type CommandInfo,
  type CompleteEntry,
  type ContextItem,
} from "../lib/chat";

export interface Chip {
  id: string;
  kind: "file" | "dir" | "web" | "docs" | "image" | "knowledge";
  ref: string;
  label: string;
  preview?: string;
}

interface PopupState {
  kind: "at" | "slash" | "hash";
  query: string;
  start: number; // 触发 token 在文本中的起始位置
  items: PopupItem[];
  selected: number;
}

interface PopupItem {
  label: string;
  detail?: string;
  badge?: string;
  section?: string;
  apply: () => void;
}

const MODEL_PRESETS = [
  { provider: "anthropic", model: "claude-sonnet-4-5-20250929", label: "Claude Sonnet" },
  { provider: "openai", model: "gpt-5.4", label: "GPT (Codex)" },
  { provider: "xai", model: "grok-build-0.1", label: "Grok Build" },
  { provider: "kimi-for-coding", model: "kimi-for-coding", label: "Kimi Code" },
];

const KNOWLEDGE_TARGETS = [
  { ref: ".agents/rules/", label: "写入项目规范", detail: ".agents/rules/（入 git 共享）" },
  { ref: "~/.agents/rules/", label: "写入全局规范", detail: "~/.agents/rules/（个人全部项目）" },
  { ref: ".kxen/memory/", label: "写入本地 memory", detail: ".kxen/memory/（本机，gitignored）" },
];

let chipSeq = 0;

export default function Composer(props: {
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
  const [popup, setPopup] = createSignal<PopupState | null>(null);
  const [modelLabel, setModelLabel] = createSignal("");
  const [modelOpen, setModelOpen] = createSignal(false);
  const [commands, setCommands] = createSignal<CommandInfo[]>([]);
  let textareaRef: HTMLTextAreaElement | undefined;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  const [recording, setRecording] = createSignal(false);
  const [voiceError, setVoiceError] = createSignal("");
  let voiceSession: VoiceSession | null = null;
  let voiceBase = "";
  let voiceFinals = "";

  /** 自增高（上限 ~8 行），发送后复位。 */
  function autoGrow() {
    if (!textareaRef) return;
    textareaRef.style.height = "auto";
    textareaRef.style.height = `${Math.min(textareaRef.scrollHeight, 180)}px`;
  }

  function toggleVoice() {
    if (recording()) {
      voiceSession?.stop();
      voiceSession = null;
      setRecording(false);
      return;
    }
    setVoiceError("");
    voiceBase = text();
    voiceFinals = "";
    const session = startVoice(
      (interim) => {
        setText(voiceBase + voiceFinals + interim);
        autoGrow();
      },
      (final) => {
        voiceFinals += `${final} `;
      },
      (error) => {
        setVoiceError(error === "not-allowed" ? "麦克风权限被拒" : `语音识别错误: ${error}`);
        setRecording(false);
        voiceSession = null;
      },
    );
    if (!session) {
      setVoiceError("当前环境不支持语音识别");
      return;
    }
    voiceSession = session;
    setRecording(true);
  }

  const estimate = () => Math.ceil(text().length / 4);
  const estimateCls = () =>
    estimate() > 190_000
      ? "text-[var(--err)]"
      : estimate() > 160_000
        ? "text-[var(--warn)]"
        : "text-[var(--text-faint)]";

  onMount(async () => {
    const m = await currentModel();
    setModelLabel(`${m.provider}/${m.model}`);
    setCommands(await commandList().catch(() => []));
    textareaRef?.focus();
  });
  onCleanup(() => debounceTimer && clearTimeout(debounceTimer));

  // 切会话/新会话：清空并重聚焦
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

  /** 触发 token 检测：光标前最近的 @ / / / #，前界为行首/空白/([{（Zed 边界规则）。 */
  function detectTrigger(
    value: string,
    cursor: number,
  ): { kind: "at" | "slash" | "hash"; start: number; query: string } | null {
    let i = cursor - 1;
    while (i >= 0) {
      const c = value[i];
      if (c === "\n") {
        if (value[i + 1] === "/") {
          return { kind: "slash", start: i + 1, query: value.slice(i + 2, cursor) };
        }
        break;
      }
      if (c === "@" || c === "#" || c === "/") {
        const prev = i === 0 ? "" : value[i - 1];
        const bounded =
          i === 0 || prev === " " || prev === "\t" || prev === "(" || prev === "[" || prev === "{";
        if (!bounded) return null;
        if (c === "/" && i !== 0) return null; // / 命令只在行首
        const kind = c === "@" ? "at" : c === "/" ? "slash" : "hash";
        return { kind, start: i, query: value.slice(i + 1, cursor) };
      }
      if (c === " " && i !== cursor - 1) break; // 查询中遇到空格结束
      i--;
    }
    return null;
  }

  function openPopup(trigger: NonNullable<ReturnType<typeof detectTrigger>>) {
    if (trigger.kind === "at") {
      if (debounceTimer) clearTimeout(debounceTimer);
      debounceTimer = setTimeout(async () => {
        const hits = await fsComplete(trigger.query, 12).catch(() => [] as CompleteEntry[]);
        const items = hits.map((h) => ({
          label: h.path,
          badge: h.kind === "dir" ? "dir" : undefined,
          apply: () => {
            addChip({
              kind: h.kind === "dir" ? "dir" : "file",
              ref: h.path,
              label: h.path.split("/").pop() ?? h.path,
            });
            closeTriggerToken(trigger);
          },
        }));
        setPopup({ ...trigger, items, selected: 0 });
      }, 200);
    } else if (trigger.kind === "slash") {
      const q = trigger.query.toLowerCase();
      const matched = commands().filter((c) => c.name.toLowerCase().includes(q));
      const toItem = (c: (typeof matched)[number], section: string) => ({
        label: `/${c.name}${c.argument_hint ? ` ${c.argument_hint}` : ""}`,
        detail: c.description,
        badge: c.kind === "skill" ? "skill" : undefined,
        section,
        apply: () => applyCommand(c, trigger),
      });
      const cmds = matched
        .filter((c) => c.kind !== "skill")
        .slice(0, 6)
        .map((c) => toItem(c, "命令"));
      const skills = matched
        .filter((c) => c.kind === "skill")
        .slice(0, 6)
        .map((c) => toItem(c, "skills"));
      setPopup({ ...trigger, items: [...cmds, ...skills], selected: 0 });
    } else {
      const q = trigger.query.toLowerCase();
      const items = KNOWLEDGE_TARGETS.filter((k) => k.label.toLowerCase().includes(q)).map((k) => ({
        label: k.label,
        detail: k.detail,
        badge: "knowledge",
        apply: () => {
          addChip({ kind: "knowledge", ref: k.ref, label: k.label });
          closeTriggerToken(trigger);
        },
      }));
      setPopup({ ...trigger, items, selected: 0 });
    }
  }

  function closeTriggerToken(trigger: { start: number }) {
    // 移除触发 token（从 start 到光标）
    const cursor = textareaRef?.selectionStart ?? text().length;
    setText((prev) => prev.slice(0, trigger.start) + prev.slice(cursor));
    setPopup(null);
    textareaRef?.focus();
  }

  function applyCommand(cmd: CommandInfo, trigger: { start: number }) {
    setPopup(null);
    // 模板命令：直接把 /name 留在文本首部（连同参数发送，后端/模型理解）
    // 内置动作命令（clear/abort/doctor/model）由 Session 处理；这里统一保留文本
    const cursor = textareaRef?.selectionStart ?? text().length;
    setText((prev) => prev.slice(0, trigger.start) + `/${cmd.name} ` + prev.slice(cursor));
    textareaRef?.focus();
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
    }
    if (e.key === "Backspace" && text() === "" && chips().length > 0) {
      removeChip(chips()[chips().length - 1].id);
    }
  }

  function onPaste(e: ClipboardEvent) {
    const items = e.clipboardData?.items ?? [];
    for (const item of items) {
      if (!item.type.startsWith("image/")) continue;
      e.preventDefault();
      const file = item.getAsFile();
      if (!file) continue;
      const reader = new FileReader();
      reader.onload = () => {
        const dataUrl = String(reader.result);
        const base64 = dataUrl.split(",")[1] ?? "";
        addChip({
          kind: "image",
          ref: dataUrl,
          label: `图片 ${file.type.split("/")[1]?.toUpperCase() ?? ""}`,
          preview: dataUrl,
        });
        // 缓存 base64 供发送
        images.set(dataUrl, { media_type: file.type, data: base64 });
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
        if (c.kind === "knowledge") return { type: "file", path: c.ref };
        return { type: "file", path: c.ref };
      });
    const imageParts = chips()
      .filter((c) => c.kind === "image")
      .map((c) => images.get(c.ref))
      .filter((i): i is { media_type: string; data: string } => !!i);
    // knowledge chip 转为提示词指示
    const knowledgeNote = chips()
      .filter((c) => c.kind === "knowledge")
      .map(
        (c) => `（请把本次相关经验沉淀到 ${c.ref}，frontmatter 带 type/description，写前给我确认）`,
      )
      .join("\n");
    if (recording()) toggleVoice();
    props.onSend(knowledgeNote ? `${value}\n${knowledgeNote}` : value, context, imageParts);
    setText("");
    setChips([]);
    autoGrow();
  }

  return (
    <div class="relative">
      {/* 弹窗（向上展开，origin-aware：从 composer 底部长出） */}
      <Show when={popup() && popup()!.items.length > 0}>
        <div class="composer-popup absolute bottom-full left-0 right-0 mb-1.5 rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] shadow-xl shadow-black/30 overflow-hidden z-20">
          <For each={popup()!.items}>
            {(item, i) => (
              <>
                <Show
                  when={
                    item.section && (i() === 0 || popup()!.items[i() - 1].section !== item.section)
                  }
                >
                  <div class="popup-section">{item.section}</div>
                </Show>
                <button
                  class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs"
                  classList={{
                    "bg-[var(--bg-overlay)]": i() === popup()!.selected,
                    "text-[var(--text-dim)]": i() !== popup()!.selected,
                  }}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    item.apply();
                  }}
                >
                  <span class="truncate flex-1 font-mono">{item.label}</span>
                  <Show when={item.detail}>
                    <span class="text-[10px] text-[var(--text-faint)] truncate max-w-[40%]">
                      {item.detail}
                    </span>
                  </Show>
                  <Show when={item.badge}>
                    <span class="text-[9px] px-1 rounded border border-[var(--border)] text-[var(--text-faint)]">
                      {item.badge}
                    </span>
                  </Show>
                </button>
              </>
            )}
          </For>
        </div>
      </Show>

      {/* 整卡 composer：chips + 自增高 textarea + 内嵌 action bar（Cursor 形态） */}
      <div class="composer-card rounded-xl border border-[var(--border)] bg-[var(--bg-raised)] focus-within:border-[var(--accent)] focus-within:shadow-[0_0_0_3px_rgba(99,102,241,0.15)]">
        {/* chips 行 */}
        <Show when={chips().length > 0}>
          <div class="flex flex-wrap gap-1.5 px-2 pt-2">
            <For each={chips()}>
              {(chip) => (
                <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md border border-[var(--border)] bg-[var(--bg-raised)] text-[11px] text-[var(--text-dim)]">
                  {chip.kind === "file" && <FileText size={11} />}
                  {chip.kind === "dir" && <Folder size={11} />}
                  {(chip.kind === "web" || chip.kind === "docs") && <Globe size={11} />}
                  {chip.kind === "image" &&
                    (chip.preview ? (
                      <img src={chip.preview} class="w-4 h-4 rounded object-cover" alt="" />
                    ) : (
                      <ImageIcon size={11} />
                    ))}
                  {chip.kind === "knowledge" && <Plus size={11} />}
                  <span class="max-w-[180px] truncate">{chip.label}</span>
                  <button
                    class="text-[var(--text-faint)] hover:text-[var(--err)]"
                    onClick={() => removeChip(chip.id)}
                  >
                    <X size={11} />
                  </button>
                </span>
              )}
            </For>
          </div>
        </Show>

        <textarea
          ref={(el) => (textareaRef = el)}
          class="w-full bg-transparent px-3 py-2 text-sm resize-none focus:outline-none placeholder:text-[var(--text-faint)] overflow-y-auto"
          rows={2}
          placeholder="输入消息，@ 引用文件，/ 命令，# 沉淀知识，Enter 发送"
          value={text()}
          onInput={onInput}
          onKeyDown={onKeyDown}
          onPaste={onPaste}
        />

        {/* action bar 内嵌卡底：左 附件/语音，右 tokens 预估 + 模型 pill + 发送/停止 */}
        <div class="flex items-center gap-1.5 px-2 pb-2">
          <button
            class="pressable p-1.5 rounded-md text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
            title="附件（@ 文件 / 粘贴图片）"
            onClick={() => textareaRef?.focus()}
          >
            <Plus size={15} />
          </button>
          <button
            class="pressable p-1.5 rounded-md disabled:opacity-30"
            classList={{
              "text-[var(--err)] animate-pulse": recording(),
              "text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60": !recording(),
            }}
            title={
              speechSupported()
                ? recording()
                  ? "停止语音输入"
                  : "语音输入（Apple 本地识别）"
                : "当前环境不支持语音识别"
            }
            disabled={!speechSupported()}
            onClick={toggleVoice}
          >
            {recording() ? <MicOff size={15} /> : <Mic size={15} />}
          </button>
          <Show when={voiceError()}>
            <span class="text-[10px] text-[var(--err)]">{voiceError()}</span>
          </Show>
          <span class={`text-[10px] tabular-nums ml-auto ${estimateCls()}`}>~{estimate()} tok</span>
          <div class="relative">
            <button
              class="pressable flex items-center gap-1 px-2 py-1 rounded-md text-xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60 border border-[var(--border)]"
              onClick={() => setModelOpen(!modelOpen())}
            >
              <span class="max-w-[140px] truncate">
                {MODEL_PRESETS.find((p) => `${p.provider}/${p.model}` === modelLabel())?.label ??
                  modelLabel()}
              </span>
              <ChevronDown size={12} />
            </button>
            <Show when={modelOpen()}>
              <div class="composer-popup absolute bottom-full right-0 mb-1.5 w-48 rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] shadow-xl shadow-black/30 overflow-hidden z-20">
                {MODEL_PRESETS.map((p) => (
                  <button
                    class="w-full px-3 py-1.5 text-left text-xs hover:bg-[var(--bg-overlay)]"
                    classList={{
                      "text-[var(--accent-hover)]": `${p.provider}/${p.model}` === modelLabel(),
                    }}
                    onClick={() => {
                      void setModel(p.provider, p.model);
                      setModelLabel(`${p.provider}/${p.model}`);
                      setModelOpen(false);
                    }}
                  >
                    <div class="font-medium">{p.label}</div>
                    <div class="text-[10px] text-[var(--text-faint)]">{p.provider}</div>
                  </button>
                ))}
              </div>
            </Show>
          </div>
          <button
            class="pressable h-8 w-8 rounded-lg flex items-center justify-center text-[var(--accent-contrast)] disabled:opacity-40"
            classList={{
              "bg-[var(--err)] hover:opacity-90": props.streaming(),
              "bg-[var(--accent)] hover:bg-[var(--accent-hover)]": !props.streaming(),
            }}
            onClick={() => (props.streaming() ? props.onStop() : void send())}
            disabled={!props.streaming() && !text().trim() && chips().length === 0}
            title={props.streaming() ? "停止" : "发送"}
          >
            {props.streaming() ? <Square size={13} /> : <Send size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}
