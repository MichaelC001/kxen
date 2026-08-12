import { createEffect, createSignal, onCleanup, Show, type Accessor, type Setter } from "solid-js";
import { activeSessionId } from "../../lib/state";
import { handlePopupKey } from "./popup-keys";
import type { PopupState, Trigger } from "./triggers";
import type { RowChip } from "./RowChips";
import AutoSuggestPanel from "./AutoSuggestPanel";
import * as auto from "./auto-suggest";

type Popup = (PopupState & Trigger) | null;

export function createComposerAutoSuggest(opts: {
  text: Accessor<string>;
  textarea: Accessor<HTMLTextAreaElement | undefined>;
  popup: Accessor<Popup>;
  setPopup: Setter<Popup>;
  rowChips: Accessor<RowChip[]>;
  pushChip: (chip: Omit<RowChip, "id">) => void;
  streaming: Accessor<boolean>;
  recording: Accessor<boolean>;
  insertText: (text: string) => void;
  closePopupIfCaretOut: () => void;
}) {
  let composing = false;
  let imeLockUntil = 0;
  const [focused, setFocused] = createSignal(false);
  const imeLocked = (event?: KeyboardEvent) =>
    composing || Boolean(event?.isComposing) || event?.keyCode === 229 || Date.now() < imeLockUntil;
  const controller = auto.createAutoSuggest({
    text: opts.text,
    sessionId: activeSessionId,
    selectedPaths: () => auto.selectedSuggestionPaths(opts.rowChips()),
    caretAtEnd: () => {
      const textarea = opts.textarea();
      return Boolean(
        textarea &&
        textarea.selectionStart === opts.text().length &&
        textarea.selectionEnd === opts.text().length,
      );
    },
    blocked: () => !focused() || Boolean(opts.popup()) || opts.streaming() || opts.recording(),
    imeLocked,
    addFile: (path) => auto.addSuggestedFile(path, opts.rowChips(), opts.pushChip),
    insertText: (value) => opts.insertText(`${opts.text().trim() ? " " : ""}${value}`),
    focus: () => opts.textarea()?.focus(),
  });
  onCleanup(controller.dispose);
  createEffect(() => {
    opts.text();
    opts.popup();
    opts.recording();
    opts.streaming();
    focused();
    activeSessionId();
    controller.run();
  });

  return {
    ...controller,
    handleKeyDown(event: KeyboardEvent) {
      const popup = opts.popup();
      if (popup && imeLocked(event)) return true;
      if (popup && handlePopupKey(event, popup, opts.setPopup)) return true;
      return !popup && controller.handleKey(event);
    },
    canSendEnter: (event: KeyboardEvent) => !imeLocked(event),
    onCaretMove() {
      opts.closePopupIfCaretOut();
      controller.run();
    },
    onBlur() {
      setFocused(false);
      opts.setPopup(null);
      controller.hide();
    },
    onFocus() {
      setFocused(true);
      controller.run();
    },
    onCompositionStart() {
      composing = true;
      controller.run();
    },
    onCompositionEnd() {
      composing = false;
      imeLockUntil = Date.now() + 50;
      controller.run();
    },
  };
}

export type ComposerAutoSuggest = ReturnType<typeof createComposerAutoSuggest>;

export function ComposerAutoSuggestPanel(props: {
  popup: Accessor<Popup>;
  controller: ComposerAutoSuggest;
}) {
  return (
    <Show when={!props.popup() && props.controller.state()}>
      {(state) => (
        <AutoSuggestPanel
          state={state()}
          onHover={props.controller.setSelected}
          onApply={props.controller.apply}
        />
      )}
    </Show>
  );
}
