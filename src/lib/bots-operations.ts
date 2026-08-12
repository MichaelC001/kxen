import { client } from "./client";
import type { BotMemoryState, BotRecoverySnapshot } from "./bots-types";

export function botMemoryList(botId: string) {
  return client.rpc<BotMemoryState>("bot.memory.list", { bot_id: botId });
}
export function botMemoryCreate(
  botId: string,
  itemId: string,
  kind: string,
  content: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc("bot.memory.create", {
    bot_id: botId,
    item_id: itemId,
    kind,
    content,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botMemoryRevise(
  botId: string,
  itemId: string,
  expectedItemVersion: number,
  content: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc("bot.memory.revise", {
    bot_id: botId,
    item_id: itemId,
    expected_item_version: expectedItemVersion,
    content,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botMemoryRemove(
  botId: string,
  itemId: string,
  expectedItemVersion: number,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc("bot.memory.remove", {
    bot_id: botId,
    item_id: itemId,
    expected_item_version: expectedItemVersion,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}

export function botRecoveryInspect() {
  return client.rpc<BotRecoverySnapshot>("bot.recovery.inspect");
}
export function botRecoveryRepair(
  kind: string,
  aggregateId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc("bot.recovery.repair", {
    kind,
    aggregate_id: aggregateId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botRecoveryClear(
  kind: string,
  aggregateId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc("bot.recovery.clear", {
    kind,
    aggregate_id: aggregateId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botArtifactGet(artifactId: string) {
  return client.rpc("bot.artifact.get", { artifact_id: artifactId });
}
export function botArtifactTrash(artifactId: string) {
  return client.rpc("bot.artifact.trash", { artifact_id: artifactId });
}
export function botArtifactRestore(artifactId: string) {
  return client.rpc("bot.artifact.restore", { artifact_id: artifactId });
}
