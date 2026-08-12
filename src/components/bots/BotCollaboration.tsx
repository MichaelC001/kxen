import { For, Show, createEffect, createSignal } from "solid-js";
import {
  botConversationArchive,
  botConversationGet,
  botConversationList,
  botConversationPause,
  botConversationPost,
  botConversationResume,
  botDirectOpen,
  botGroupAddMember,
  botGroupCreate,
  botGroupRemoveMember,
  botGroupSetModerator,
  botGroupStop,
  botList,
  botTaskCancel,
  newBotId,
  type BotConversation,
  type BotSummary,
} from "../../lib/bots";
import { flashErr, flashOk } from "../../lib/flash";
import { formatError } from "../../lib/error-text";
import BotCollaborationCreate from "./BotCollaborationCreate";
import BotConversationDetail from "./BotConversationDetail";
import { Panel, shortId, statusClass, type RefreshProps } from "./shared";

export default function BotCollaboration(props: RefreshProps) {
  const [bots, setBots] = createSignal<BotSummary[]>([]);
  const [conversations, setConversations] = createSignal<BotConversation[]>([]);
  const [selectedId, setSelectedId] = createSignal("");
  const [detail, setDetail] = createSignal<BotConversation | null>(null);
  const [members, setMembers] = createSignal<string[]>([]);
  const [moderator, setModerator] = createSignal("");
  const [left, setLeft] = createSignal("");
  const [right, setRight] = createSignal("");
  const [instruction, setInstruction] = createSignal("");
  const [mentions, setMentions] = createSignal<string[]>([]);
  const [everyone, setEveryone] = createSignal(false);
  const [addBotId, setAddBotId] = createSignal("");
  const [acting, setActing] = createSignal(false);
  const [loadErr, setLoadErr] = createSignal("");
  let loadSeq = 0;

  const reload = async () => {
    const seq = ++loadSeq;
    try {
      const [botItems, convItems] = await Promise.all([
        botList(),
        botConversationList(undefined, true),
      ]);
      if (seq !== loadSeq) return;
      setBots(botItems.filter((bot) => bot.lifecycle === "active"));
      const visible = convItems.filter((conversation) =>
        ["bot_group", "bot_direct"].includes(conversation.kind),
      );
      setConversations(visible);
      const wanted = selectedId() || visible[0]?.conversation_id || "";
      setSelectedId(wanted);
      setDetail(wanted ? await botConversationGet(wanted) : null);
      setLoadErr("");
    } catch (error) {
      if (seq === loadSeq) setLoadErr(formatError(error));
    }
  };
  createEffect(() => {
    void props.epoch;
    void reload();
  });
  const select = (id: string) => {
    setSelectedId(id);
    setMentions([]);
    setEveryone(false);
    void reload();
  };
  const act = async (job: () => Promise<unknown>, label: string) => {
    if (acting()) return;
    setActing(true);
    try {
      await job();
      await reload();
      props.onChanged();
      flashOk(label);
    } catch (error) {
      flashErr(`${label}失败：${formatError(error)}`);
    } finally {
      setActing(false);
    }
  };
  const toggleMember = (botId: string) => {
    const current = members();
    if (current.includes(botId)) {
      setMembers(current.filter((id) => id !== botId));
      if (moderator() === botId) setModerator("");
    } else if (current.length < 6) {
      setMembers([...current, botId]);
    }
  };
  const createGroup = () => {
    if (!(members().length >= 2 && members().length <= 6) || !moderator()) return;
    const id = newBotId("bconv");
    void act(async () => {
      await botGroupCreate(id, members(), moderator(), newBotId("idem"));
      setSelectedId(id);
      setMembers([]);
      setModerator("");
    }, "Bot Group 已创建");
  };
  const openDirect = () => {
    if (!left() || !right() || left() === right()) return;
    void act(async () => {
      const conversation = await botDirectOpen(left(), right(), newBotId("idem"));
      setSelectedId(conversation.conversation_id);
    }, "Bot Direct 已打开");
  };
  const post = () => {
    const conversation = detail();
    const text = instruction().trim();
    if (!conversation || conversation.kind !== "bot_group" || !text) return;
    void act(async () => {
      await botConversationPost(
        conversation.conversation_id,
        newBotId("bmsg"),
        [{ kind: "text", text }],
        conversation.event_version,
        newBotId("idem"),
        { ...(everyone() ? {} : { mentions: mentions() }), everyone: everyone() },
      );
      setInstruction("");
    }, "Group 指令已投递");
  };
  const mutateConversation = (kind: "pause" | "resume" | "archive" | "stop") => {
    const conversation = detail();
    if (!conversation) return;
    const key = newBotId("idem");
    const jobs = {
      pause: () =>
        botConversationPause(conversation.conversation_id, conversation.event_version, key),
      resume: () =>
        botConversationResume(conversation.conversation_id, conversation.event_version, key),
      archive: () =>
        botConversationArchive(conversation.conversation_id, conversation.event_version, key),
      stop: () => botGroupStop(conversation.conversation_id, conversation.event_version, key),
    };
    void act(jobs[kind], kind === "stop" ? "Bot Group 已停止" : `Conversation ${kind} 已提交`);
  };
  const addMember = () => {
    const conversation = detail();
    if (!conversation || !addBotId()) return;
    void act(
      () =>
        botGroupAddMember(
          conversation.conversation_id,
          addBotId(),
          conversation.event_version,
          newBotId("idem"),
        ),
      "成员已加入",
    );
  };
  const removeMember = (botId: string) => {
    const conversation = detail();
    if (!conversation) return;
    void act(
      () =>
        botGroupRemoveMember(
          conversation.conversation_id,
          botId,
          conversation.event_version,
          newBotId("idem"),
        ),
      "成员已移除",
    );
  };
  const setGroupModerator = (botId: string) => {
    const conversation = detail();
    if (!conversation) return;
    void act(
      () =>
        botGroupSetModerator(
          conversation.conversation_id,
          botId,
          conversation.event_version,
          newBotId("idem"),
        ),
      "Moderator 已更新",
    );
  };
  const cancelTask = (taskId: string) => {
    const conversation = detail();
    if (!conversation) return;
    void act(
      () =>
        botTaskCancel(
          conversation.conversation_id,
          taskId,
          conversation.event_version,
          newBotId("idem"),
        ),
      "Task 已取消",
    );
  };

  return (
    <div class="space-y-4">
      <BotCollaborationCreate
        bots={bots()}
        members={members()}
        moderator={moderator()}
        left={left()}
        right={right()}
        acting={acting()}
        toggleMember={toggleMember}
        setModerator={setModerator}
        setLeft={setLeft}
        setRight={setRight}
        createGroup={createGroup}
        openDirect={openDirect}
      />
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <Panel title="协作空间" detail="这里是 Bot collaboration timeline，不是多人聊天。">
          <Show when={loadErr()}>
            <p class="text-xs text-[var(--err)] mb-2">{loadErr()}</p>
          </Show>
          <div class="space-y-2">
            <For
              each={conversations()}
              fallback={
                <p class="text-xs text-[var(--text-faint)]">暂无 Bot-to-Bot Conversation。</p>
              }
            >
              {(conversation) => (
                <button
                  class="pressable w-full text-left rounded border border-[var(--border)] p-2"
                  classList={{
                    "border-[var(--accent)]": selectedId() === conversation.conversation_id,
                  }}
                  onClick={() => select(conversation.conversation_id)}
                >
                  <div class="flex gap-2">
                    <span class="text-xs">
                      {conversation.kind === "bot_group" ? "Group" : "Direct"}
                    </span>
                    <span class={`ml-auto text-2xs ${statusClass(conversation.lifecycle)}`}>
                      {conversation.lifecycle}
                    </span>
                  </div>
                  <div class="font-mono text-2xs text-[var(--text-faint)]">
                    {shortId(conversation.conversation_id)}
                  </div>
                </button>
              )}
            </For>
          </div>
        </Panel>
        <div class="lg:col-span-2 space-y-4">
          <Show
            when={detail()}
            fallback={
              <Panel title="Conversation">
                <p class="text-xs text-[var(--text-faint)]">选择一个协作空间。</p>
              </Panel>
            }
          >
            {(conversation) => (
              <BotConversationDetail
                conversation={conversation()}
                bots={bots()}
                acting={acting()}
                addBotId={addBotId()}
                instruction={instruction()}
                mentions={mentions()}
                everyone={everyone()}
                setAddBotId={setAddBotId}
                setInstruction={setInstruction}
                setMentions={setMentions}
                setEveryone={setEveryone}
                addMember={addMember}
                removeMember={removeMember}
                setModerator={setGroupModerator}
                mutate={mutateConversation}
                post={post}
                cancelTask={cancelTask}
              />
            )}
          </Show>
        </div>
      </div>
    </div>
  );
}
