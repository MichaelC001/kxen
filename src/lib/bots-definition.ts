import { client } from "./client";
import type { BotDefinition, BotState, BotSummary, BuilderState } from "./bots-types";

export function botList(includeTrashed = false) {
  return client.rpc<BotSummary[]>("bot.list", { include_trashed: includeTrashed });
}
export function botGet(botId: string) {
  return client.rpc<BotState>("bot.get", { bot_id: botId });
}
export function botCreate(botId: string, displayName: string, idempotencyKey: string) {
  return client.rpc<BotState>("bot.create", {
    bot_id: botId,
    display_name: displayName,
    idempotency_key: idempotencyKey,
  });
}
export function botDuplicate(
  sourceBotId: string,
  botId: string,
  displayName: string,
  idempotencyKey: string,
  revisionId?: string,
) {
  return client.rpc<BotState>("bot.duplicate", {
    source_bot_id: sourceBotId,
    bot_id: botId,
    display_name: displayName,
    idempotency_key: idempotencyKey,
    ...(revisionId ? { revision_id: revisionId } : {}),
  });
}
export function botDraftGet(botId: string) {
  return client.rpc("bot.draft.get", { bot_id: botId });
}
export function botDraftPatch(
  botId: string,
  expectedVersion: number,
  expectedDraftVersion: number,
  definition: BotDefinition,
  idempotencyKey: string,
) {
  return client.rpc<BotState>("bot.draft.patch", {
    bot_id: botId,
    expected_version: expectedVersion,
    expected_draft_version: expectedDraftVersion,
    definition,
    idempotency_key: idempotencyKey,
  });
}
export function botValidate(builderSessionId: string, idempotencyKey: string) {
  return client.rpc<BuilderState>("bot.validate", {
    builder_session_id: builderSessionId,
    idempotency_key: idempotencyKey,
  });
}
export function botPublish(builderSessionId: string, reviewHash: string, idempotencyKey: string) {
  return client.rpc<BotState>("bot.publish", {
    builder_session_id: builderSessionId,
    review_hash: reviewHash,
    idempotency_key: idempotencyKey,
  });
}

export const botPause = (botId: string, expectedVersion: number, idempotencyKey: string) =>
  client.rpc<BotState>("bot.pause", {
    bot_id: botId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
export const botResume = (botId: string, expectedVersion: number, idempotencyKey: string) =>
  client.rpc<BotState>("bot.resume", {
    bot_id: botId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
export const botArchive = (botId: string, expectedVersion: number, idempotencyKey: string) =>
  client.rpc<BotState>("bot.archive", {
    bot_id: botId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
export const botTrash = (botId: string, expectedVersion: number, idempotencyKey: string) =>
  client.rpc<BotState>("bot.trash", {
    bot_id: botId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
export const botRestore = (botId: string, expectedVersion: number, idempotencyKey: string) =>
  client.rpc<BotState>("bot.restore", {
    bot_id: botId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
