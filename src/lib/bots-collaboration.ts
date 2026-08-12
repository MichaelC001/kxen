import { client } from "./client";
import type { BotConversation, BotMessagePart, BotPostOptions, BotTask } from "./bots-types";

export function botConversationList(kind?: string, includeArchived = false) {
  return client.rpc<BotConversation[]>("bot.conversation.list", {
    ...(kind ? { kind } : {}),
    include_archived: includeArchived,
  });
}
export function botConversationGet(conversationId: string) {
  return client.rpc<BotConversation>("bot.conversation.get", { conversation_id: conversationId });
}
export function botConversationPost(
  conversationId: string,
  messageId: string,
  parts: BotMessagePart[],
  expectedVersion: number,
  idempotencyKey: string,
  options: BotPostOptions = {},
) {
  return client.rpc<BotConversation>("bot.conversation.post", {
    conversation_id: conversationId,
    message_id: messageId,
    parts,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
    ...options,
  });
}
export function botConversationPause(
  conversationId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotConversation>("bot.conversation.pause", {
    conversation_id: conversationId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botConversationResume(
  conversationId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotConversation>("bot.conversation.resume", {
    conversation_id: conversationId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botConversationArchive(
  conversationId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotConversation>("bot.conversation.archive", {
    conversation_id: conversationId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botDirectOpen(leftBotId: string, rightBotId: string, idempotencyKey: string) {
  return client.rpc<BotConversation>("bot.direct.open", {
    left_bot_id: leftBotId,
    right_bot_id: rightBotId,
    idempotency_key: idempotencyKey,
  });
}
export function botGroupCreate(
  conversationId: string,
  botIds: string[],
  moderatorBotId: string,
  idempotencyKey: string,
) {
  return client.rpc<BotConversation>("bot.group.create", {
    conversation_id: conversationId,
    bot_ids: botIds,
    moderator_bot_id: moderatorBotId,
    idempotency_key: idempotencyKey,
  });
}
export function botGroupAddMember(
  conversationId: string,
  botId: string,
  expectedVersion: number,
  idempotencyKey: string,
  historyVisibleFromSeq?: number,
) {
  return client.rpc<BotConversation>("bot.group.add_member", {
    conversation_id: conversationId,
    bot_id: botId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
    ...(historyVisibleFromSeq === undefined
      ? {}
      : { history_visible_from_seq: historyVisibleFromSeq }),
  });
}
export function botGroupRemoveMember(
  conversationId: string,
  botId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotConversation>("bot.group.remove_member", {
    conversation_id: conversationId,
    bot_id: botId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botGroupSetModerator(
  conversationId: string,
  botId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotConversation>("bot.group.set_moderator", {
    conversation_id: conversationId,
    bot_id: botId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botGroupStop(
  conversationId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotConversation>("bot.group.stop", {
    conversation_id: conversationId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}

export function botTaskList(filters: { conversation_id?: string; owner_bot_id?: string } = {}) {
  return client.rpc<BotTask[]>("bot.task.list", filters);
}
export function botTaskGet(taskId: string) {
  return client.rpc("bot.task.get", { task_id: taskId });
}
export function botTaskCancel(
  conversationId: string,
  taskId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc("bot.task.cancel", {
    conversation_id: conversationId,
    task_id: taskId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botTaskReassign(
  conversationId: string,
  taskId: string,
  botId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc("bot.task.reassign", {
    conversation_id: conversationId,
    task_id: taskId,
    bot_id: botId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
