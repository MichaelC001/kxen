import { For, Show } from "solid-js";
import type { BotMemoryState, BotState } from "../../lib/bots";
import { editableBotDefinition, publishedBotDefinition } from "./bot-definition";
import { actionClass, fieldClass, Panel, primaryClass, shortId, statusClass } from "./shared";

interface BotLibraryDetailProps {
  state: BotState;
  memory: BotMemoryState | null;
  acting: boolean;
  prompt: string;
  memoryText: string;
  memoryKind: string;
  editingMemory: string;
  lifecycle: (kind: "pause" | "resume" | "archive" | "trash" | "restore") => void;
  duplicate: () => void;
  run: () => void;
  saveMemory: () => void;
  removeMemory: (itemId: string, version: number) => void;
  build: () => void;
  setPrompt: (value: string) => void;
  setMemoryText: (value: string) => void;
  setMemoryKind: (value: string) => void;
  setEditingMemory: (value: string) => void;
}

export default function BotLibraryDetail(props: BotLibraryDetailProps) {
  const definition = () => editableBotDefinition(props.state);
  const runDefinition = () => publishedBotDefinition(props.state);
  return (
    <>
      <Panel
        title={definition()?.display_name || props.state.bot_id}
        detail={definition()?.description || "尚无描述"}
      >
        <div class="grid grid-cols-2 gap-3 text-xs">
          <div>
            <span class="text-[var(--text-faint)]">状态</span>
            <div class={statusClass(props.state.lifecycle)}>{props.state.lifecycle}</div>
          </div>
          <div>
            <span class="text-[var(--text-faint)]">Revision</span>
            <div class="font-mono">
              {props.state.current_revision_id
                ? shortId(props.state.current_revision_id)
                : "未发布"}
            </div>
          </div>
          <div class="col-span-2">
            <span class="text-[var(--text-faint)]">目标</span>
            <div class="selectable whitespace-pre-wrap">{definition()?.objective || "未设置"}</div>
          </div>
          <div class="col-span-2">
            <span class="text-[var(--text-faint)]">Capabilities</span>
            <div>{definition()?.capabilities.join(", ") || "无"}</div>
          </div>
        </div>
        <LifecycleActions
          state={props.state}
          acting={props.acting}
          lifecycle={props.lifecycle}
          duplicate={props.duplicate}
          build={props.build}
        />
      </Panel>

      <Show when={props.state.lifecycle === "active"}>
        <Panel
          title="手动运行"
          detail={`创建持久化 BotRun。输入契约：${runDefinition()?.input_contract.content_type ?? "text/plain"}${runDefinition()?.input_contract.required_fields.length ? `，必填 ${runDefinition()!.input_contract.required_fields.join(", ")}` : ""}。`}
        >
          <textarea
            class={`${fieldClass} min-h-20`}
            value={props.prompt}
            onInput={(event) => props.setPrompt(event.currentTarget.value)}
            placeholder="描述本次要完成的工作"
          />
          <button
            class={`${primaryClass} mt-2`}
            disabled={props.acting || !props.prompt.trim()}
            onClick={props.run}
          >
            运行 Bot
          </button>
        </Panel>
      </Show>

      <Panel
        title="Bot Memory"
        detail="Owner 或获授 bot_memory capability 的 Bot 可写入结构化记忆。Conversation 不会自动写入，凭据和 secret 会被拒绝。"
      >
        <div class="space-y-2 mb-3">
          <For
            each={Object.values(props.memory?.items ?? {})}
            fallback={<p class="text-xs text-[var(--text-faint)]">暂无 Memory。</p>}
          >
            {(item) => (
              <div class="rounded border border-[var(--border)] p-2 text-xs">
                <div class="flex gap-2">
                  <span class="text-[var(--accent-hover)]">{item.kind}</span>
                  <span class="text-[var(--text-faint)]">v{item.version}</span>
                </div>
                <div class="selectable whitespace-pre-wrap my-1">{item.content}</div>
                <div class="flex gap-2">
                  <button
                    class={actionClass}
                    onClick={() => {
                      props.setEditingMemory(item.item_id);
                      props.setMemoryKind(item.kind);
                      props.setMemoryText(item.content);
                    }}
                  >
                    编辑
                  </button>
                  <button
                    class={actionClass}
                    disabled={props.acting}
                    onClick={() => props.removeMemory(item.item_id, item.version)}
                  >
                    删除
                  </button>
                </div>
              </div>
            )}
          </For>
        </div>
        <div class="flex gap-2">
          <select
            class="form-select"
            value={props.memoryKind}
            disabled={Boolean(props.editingMemory)}
            onChange={(event) => props.setMemoryKind(event.currentTarget.value)}
          >
            <option value="fact">fact</option>
            <option value="preference">preference</option>
            <option value="procedure">procedure</option>
            <option value="constraint">constraint</option>
          </select>
          <input
            class={fieldClass}
            value={props.memoryText}
            onInput={(event) => props.setMemoryText(event.currentTarget.value)}
            placeholder="明确、非敏感的记忆内容"
          />
          <button
            class={primaryClass}
            disabled={props.acting || !props.memoryText.trim()}
            onClick={props.saveMemory}
          >
            {props.editingMemory ? "保存" : "添加"}
          </button>
          <Show when={props.editingMemory}>
            <button
              class={actionClass}
              onClick={() => {
                props.setEditingMemory("");
                props.setMemoryText("");
              }}
            >
              取消
            </button>
          </Show>
        </div>
      </Panel>
    </>
  );
}

function LifecycleActions(props: {
  state: BotState;
  acting: boolean;
  lifecycle: BotLibraryDetailProps["lifecycle"];
  duplicate: () => void;
  build: () => void;
}) {
  return (
    <div class="flex flex-wrap gap-2 mt-4">
      <Show when={props.state.lifecycle === "active"}>
        <button
          class={actionClass}
          disabled={props.acting}
          onClick={() => props.lifecycle("pause")}
        >
          Pause
        </button>
      </Show>
      <Show when={props.state.lifecycle === "paused"}>
        <button
          class={actionClass}
          disabled={props.acting}
          onClick={() => props.lifecycle("resume")}
        >
          Resume
        </button>
      </Show>
      <Show when={["active", "paused"].includes(props.state.lifecycle)}>
        <button
          class={actionClass}
          disabled={props.acting}
          onClick={() => props.lifecycle("archive")}
        >
          Archive
        </button>
      </Show>
      <Show when={props.state.lifecycle !== "trashed" && props.state.lifecycle !== "blocked"}>
        <button
          class={actionClass}
          disabled={props.acting}
          onClick={() => props.lifecycle("trash")}
        >
          移到废纸篓
        </button>
      </Show>
      <Show when={props.state.lifecycle === "trashed"}>
        <button
          class={actionClass}
          disabled={props.acting}
          onClick={() => props.lifecycle("restore")}
        >
          Restore
        </button>
      </Show>
      <button
        class={actionClass}
        disabled={
          props.acting || props.state.lifecycle === "trashed" || props.state.lifecycle === "blocked"
        }
        onClick={props.build}
      >
        与 Bot 对话编辑
      </button>
      <button
        class={actionClass}
        disabled={props.acting || !props.state.current_revision_id}
        onClick={props.duplicate}
      >
        Duplicate
      </button>
    </div>
  );
}
