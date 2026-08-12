import { createSignal } from "solid-js";
import {
  composerSuggestCancel,
  composerSuggestLocal,
  composerSuggestRemote,
  configGet,
  type ComposerSuggestion,
} from "../../lib/chat";
import type { RowChip } from "./RowChips";

export interface AutoSuggestState {
  items: ComposerSuggestion[];
  selected: number;
}

interface Dependencies {
  config: typeof configGet;
  local: typeof composerSuggestLocal;
  remote: typeof composerSuggestRemote;
  cancel: typeof composerSuggestCancel;
}

const defaults: Dependencies = {
  config: configGet,
  local: composerSuggestLocal,
  remote: composerSuggestRemote,
  cancel: composerSuggestCancel,
};

export const selectedSuggestionPaths = (chips: RowChip[]) =>
  chips.filter((chip) => chip.kind === "file" || chip.kind === "dir").map((chip) => chip.ref);

export function addSuggestedFile(
  path: string,
  chips: RowChip[],
  push: (chip: Omit<RowChip, "id">) => void,
) {
  if (!chips.some((chip) => chip.kind === "file" && chip.ref === path)) {
    push({ kind: "file", ref: path, label: path.split("/").pop() ?? path, title: path });
  }
}

export function createAutoSuggest(
  opts: {
    text: () => string;
    sessionId: () => string;
    selectedPaths: () => string[];
    caretAtEnd: () => boolean;
    blocked: () => boolean;
    imeLocked: () => boolean;
    addFile: (path: string) => void;
    insertText: (text: string) => void;
    focus: () => void;
  },
  deps: Dependencies = defaults,
) {
  const [state, setState] = createSignal<AutoSuggestState | null>(null);
  let settings = { ready: false, enabled: false, semantic: false, llm: false };
  let generation = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let remoteTimer: ReturnType<typeof setTimeout> | undefined;
  let remoteRequest: { sessionId: string; requestId: string } | undefined;
  let dismissedDraft = "";
  let requestSeq = 0;

  const cancelRemote = () => {
    const current = remoteRequest;
    remoteRequest = undefined;
    if (current) void deps.cancel(current.sessionId, current.requestId).catch(() => {});
  };

  const clearTimers = () => {
    if (timer) clearTimeout(timer);
    if (remoteTimer) clearTimeout(remoteTimer);
    timer = undefined;
    remoteTimer = undefined;
  };

  const eligible = (draft = opts.text()) =>
    settings.ready &&
    settings.enabled &&
    draft.trim().length >= 3 &&
    draft !== dismissedDraft &&
    opts.caretAtEnd() &&
    !opts.blocked() &&
    !opts.imeLocked();

  const current = (request: number, draft: string, sessionId: string) =>
    request === generation &&
    opts.text() === draft &&
    opts.sessionId() === sessionId &&
    eligible(draft);

  const merge = (primary: ComposerSuggestion[], secondary: ComposerSuggestion[]) => {
    const seen = new Set<string>();
    return [...primary, ...secondary]
      .filter((item) => !seen.has(item.id) && Boolean(seen.add(item.id)))
      .slice(0, 6);
  };

  const run = () => {
    const request = ++generation;
    clearTimers();
    cancelRemote();
    const draft = opts.text();
    const sessionId = opts.sessionId();
    if (!eligible(draft)) {
      setState(null);
      return;
    }
    setState(null);
    timer = setTimeout(async () => {
      timer = undefined;
      let local: ComposerSuggestion[] = [];
      try {
        local = (await deps.local(draft, sessionId, opts.selectedPaths(), 6)).suggestions;
      } catch {
        local = [];
      }
      if (!current(request, draft, sessionId)) return;
      setState(local.length ? { items: local, selected: 0 } : null);
      if ((!settings.semantic && !settings.llm) || !sessionId) return;
      remoteTimer = setTimeout(async () => {
        remoteTimer = undefined;
        if (!current(request, draft, sessionId)) return;
        const requestId = `suggest_${Date.now()}_${++requestSeq}`;
        remoteRequest = { sessionId, requestId };
        try {
          const response = await deps.remote(
            draft,
            sessionId,
            opts.selectedPaths(),
            local.filter((item) => item.kind === "file").map((item) => item.id),
            requestId,
            6,
          );
          if (!current(request, draft, sessionId) || remoteRequest?.requestId !== requestId) return;
          const items = merge(response.suggestions, local);
          setState(items.length ? { items, selected: 0 } : null);
        } catch {
          // Provider、网络或配置失败保持本地候选，不把自动推荐错误打断输入。
        } finally {
          if (remoteRequest?.requestId === requestId) remoteRequest = undefined;
        }
      }, 550);
    }, 350);
  };

  const hide = () => {
    generation++;
    clearTimers();
    cancelRemote();
    setState(null);
  };

  const dismiss = () => {
    dismissedDraft = opts.text();
    hide();
  };

  const apply = (index: number) => {
    const item = state()?.items[index];
    if (!item) return;
    if (item.kind === "file") opts.addFile(item.path);
    else opts.insertText(item.label);
    dismiss();
    opts.focus();
  };

  const handleKey = (event: KeyboardEvent): boolean => {
    const currentState = state();
    if (!currentState || opts.imeLocked()) return false;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const delta = event.key === "ArrowDown" ? 1 : -1;
      setState({
        ...currentState,
        selected:
          (currentState.selected + delta + currentState.items.length) % currentState.items.length,
      });
      return true;
    }
    if (event.key === "Tab") {
      event.preventDefault();
      apply(currentState.selected);
      return true;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      dismiss();
      return true;
    }
    return false;
  };

  void deps
    .config()
    .then((config) => {
      const value = config.composer_suggestions;
      settings = {
        ready: true,
        enabled: value?.enabled !== false,
        semantic: value?.semantic === true,
        llm: value?.llm === true,
      };
      run();
    })
    .catch(() => {
      settings = { ready: false, enabled: false, semantic: false, llm: false };
    });

  return {
    state,
    run,
    hide,
    dismiss,
    apply,
    setSelected: (selected: number) =>
      setState((current) => (current ? { ...current, selected } : current)),
    handleKey,
    dispose: () => {
      generation++;
      clearTimers();
      cancelRemote();
    },
  };
}
