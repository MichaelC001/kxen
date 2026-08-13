import { Show, createEffect, createSignal } from "solid-js";
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
import { createReconciledMutation } from "../../lib/async-guard";
import { formatError } from "../../lib/error-text";
import BotCollaborationCreate from "./BotCollaborationCreate";
import BotConversationList from "./BotConversationList";
import BotConversationDetail from "./BotConversationDetail";
import { conversationLifecycleAfter, findDirectConversation } from "./mutation-state";
import { Panel, type RefreshProps } from "./shared";

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
      const selected = selectedId();
      const wanted = visible.some((conversation) => conversation.conversation_id === selected)
        ? selected
        : visible[0]?.conversation_id || "";
      setSelectedId(wanted);
      if (detail()?.conversation_id !== wanted) setDetail(null);
      const state = wanted ? await botConversationGet(wanted) : null;
      if (seq !== loadSeq) return;
      setDetail(state);
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
    setDetail(null);
    setMentions([]);
    setEveryone(false);
    void reload();
  };
  const mutation = createReconciledMutation({ refresh: reload, onChanged: props.onChanged });
  const acting = mutation.pending;
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
    const botIds = [...members()];
    const moderatorId = moderator();
    void mutation.run({
      key: `group:create:${botIds.join(",")}:${moderatorId}`,
      prepare: () => ({ conversationId: newBotId("bconv"), idempotencyKey: newBotId("idem") }),
      execute: ({ conversationId, idempotencyKey }) =>
        botGroupCreate(conversationId, botIds, moderatorId, idempotencyKey),
      applied: ({ conversationId }) =>
        conversations().some((conversation) => conversation.conversation_id === conversationId),
      onApplied: ({ conversationId }) => {
        setSelectedId(conversationId);
        setDetail(null);
        setMembers([]);
        setModerator("");
      },
      okText: "Bot Group 已创建",
    });
  };
  const openDirect = () => {
    if (!left() || !right() || left() === right()) return;
    const leftBotId = left();
    const rightBotId = right();
    void mutation.run({
      key: `direct:open:${leftBotId}:${rightBotId}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => botDirectOpen(leftBotId, rightBotId, idempotencyKey),
      applied: () => Boolean(findDirectConversation(conversations(), leftBotId, rightBotId)),
      onApplied: () => {
        const opened = findDirectConversation(conversations(), leftBotId, rightBotId);
        if (opened) setSelectedId(opened.conversation_id);
        setDetail(null);
      },
      okText: "Bot Direct 已打开",
    });
  };
  const post = () => {
    const conversation = detail();
    const text = instruction().trim();
    if (!conversation || conversation.kind !== "bot_group" || !text) return;
    const conversationId = conversation.conversation_id;
    const targetMentions = [...mentions()];
    const targetEveryone = everyone();
    void mutation.run({
      key: `conversation:${conversationId}:post:${targetEveryone}:${targetMentions.join(",")}:${text}`,
      prepare: () => ({ messageId: newBotId("bmsg"), idempotencyKey: newBotId("idem") }),
      execute: ({ messageId, idempotencyKey }) => {
        const current = detail();
        if (!current || current.conversation_id !== conversationId)
          throw new Error("selected Conversation changed");
        return botConversationPost(
          conversationId,
          messageId,
          [{ kind: "text", text }],
          current.event_version,
          idempotencyKey,
          {
            ...(targetEveryone ? {} : { mentions: targetMentions }),
            everyone: targetEveryone,
          },
        );
      },
      applied: ({ messageId }) =>
        detail()?.conversation_id === conversationId &&
        Boolean(detail()?.messages.some((message) => message.message_id === messageId)),
      onApplied: () => setInstruction(""),
      okText: "Group 指令已投递",
    });
  };
  const mutateConversation = (kind: "pause" | "resume" | "archive" | "stop") => {
    const conversation = detail();
    if (!conversation) return;
    const conversationId = conversation.conversation_id;
    const expectedLifecycle = conversationLifecycleAfter(kind);
    void mutation.run({
      key: `conversation:${conversationId}:lifecycle:${kind}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => {
        const current = detail();
        if (!current || current.conversation_id !== conversationId)
          throw new Error("selected Conversation changed");
        const jobs = {
          pause: () => botConversationPause(conversationId, current.event_version, idempotencyKey),
          resume: () =>
            botConversationResume(conversationId, current.event_version, idempotencyKey),
          archive: () =>
            botConversationArchive(conversationId, current.event_version, idempotencyKey),
          stop: () => botGroupStop(conversationId, current.event_version, idempotencyKey),
        };
        return jobs[kind]();
      },
      applied: () =>
        detail()?.conversation_id === conversationId && detail()?.lifecycle === expectedLifecycle,
      okText: kind === "stop" ? "Bot Group 已停止" : `Conversation ${kind} 已提交`,
    });
  };
  const addMember = () => {
    const conversation = detail();
    const botId = addBotId();
    if (!conversation || !botId) return;
    const conversationId = conversation.conversation_id;
    void mutation.run({
      key: `conversation:${conversationId}:member:add:${botId}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => {
        const current = detail();
        if (!current || current.conversation_id !== conversationId)
          throw new Error("selected Conversation changed");
        return botGroupAddMember(conversationId, botId, current.event_version, idempotencyKey);
      },
      applied: () =>
        detail()?.conversation_id === conversationId && detail()?.members[botId]?.active === true,
      onApplied: () => setAddBotId(""),
      okText: "成员已加入",
    });
  };
  const removeMember = (botId: string) => {
    const conversation = detail();
    if (!conversation) return;
    const conversationId = conversation.conversation_id;
    void mutation.run({
      key: `conversation:${conversationId}:member:remove:${botId}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => {
        const current = detail();
        if (!current || current.conversation_id !== conversationId)
          throw new Error("selected Conversation changed");
        return botGroupRemoveMember(conversationId, botId, current.event_version, idempotencyKey);
      },
      applied: () =>
        detail()?.conversation_id === conversationId && detail()?.members[botId]?.active === false,
      okText: "成员已移除",
    });
  };
  const setGroupModerator = (botId: string) => {
    const conversation = detail();
    if (!conversation) return;
    const conversationId = conversation.conversation_id;
    void mutation.run({
      key: `conversation:${conversationId}:moderator:${botId}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => {
        const current = detail();
        if (!current || current.conversation_id !== conversationId)
          throw new Error("selected Conversation changed");
        return botGroupSetModerator(conversationId, botId, current.event_version, idempotencyKey);
      },
      applied: () =>
        detail()?.conversation_id === conversationId && detail()?.moderator_bot_id === botId,
      okText: "Moderator 已更新",
    });
  };
  const cancelTask = (taskId: string) => {
    const conversation = detail();
    if (!conversation) return;
    const conversationId = conversation.conversation_id;
    void mutation.run({
      key: `conversation:${conversationId}:task:cancel:${taskId}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => {
        const current = detail();
        if (!current || current.conversation_id !== conversationId)
          throw new Error("selected Conversation changed");
        return botTaskCancel(conversationId, taskId, current.event_version, idempotencyKey);
      },
      applied: () =>
        detail()?.conversation_id === conversationId &&
        detail()?.tasks[taskId]?.status === "canceled",
      okText: "Task 已取消",
    });
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
        <BotConversationList
          conversations={conversations()}
          selectedId={selectedId()}
          loadError={loadErr()}
          onSelect={select}
        />
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
