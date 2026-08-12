import { client } from "./client";
import type { BotRun, BuilderState } from "./bots-types";

export function botBuilderStart(
  botId: string,
  builderSessionId: string,
  goal: string,
  displayName: string,
  idempotencyKey: string,
) {
  return client.rpc<BuilderState>("bot.builder.start", {
    bot_id: botId,
    builder_session_id: builderSessionId,
    user_goal: goal,
    display_name: displayName,
    idempotency_key: idempotencyKey,
  });
}
export function botBuilderMessage(
  builderSessionId: string,
  messageId: string,
  text: string,
  idempotencyKey: string,
) {
  return client.rpc<BuilderState>("bot.builder.message", {
    builder_session_id: builderSessionId,
    message_id: messageId,
    text,
    idempotency_key: idempotencyKey,
  });
}
export function botBuilderGet(builderSessionId: string) {
  return client.rpc<BuilderState>("bot.builder.get", { builder_session_id: builderSessionId });
}
export function botBuilderGrant(
  builderSessionId: string,
  draftHash: string,
  reason: string,
  idempotencyKey: string,
) {
  return client.rpc<BuilderState>("bot.builder.grant", {
    builder_session_id: builderSessionId,
    draft_hash: draftHash,
    reason,
    idempotency_key: idempotencyKey,
  });
}
export function botBuilderTest(builderSessionId: string, runId: string, idempotencyKey: string) {
  return client.rpc<BotRun>("bot.builder.test", {
    builder_session_id: builderSessionId,
    run_id: runId,
    idempotency_key: idempotencyKey,
  });
}
export function botBuilderCancel(builderSessionId: string, idempotencyKey: string) {
  return client.rpc<BuilderState>("bot.builder.cancel", {
    builder_session_id: builderSessionId,
    idempotency_key: idempotencyKey,
  });
}

export function botRunStart(
  runId: string,
  botId: string,
  input: unknown[],
  idempotencyKey: string,
  conversationId?: string,
) {
  return client.rpc<BotRun>("bot.run.start", {
    run_id: runId,
    bot_id: botId,
    input,
    idempotency_key: idempotencyKey,
    ...(conversationId ? { conversation_id: conversationId } : {}),
  });
}
export function botRunGet(runId: string) {
  return client.rpc<BotRun>("bot.run.get", { run_id: runId });
}
export function botRunList(
  filters: { bot_id?: string; conversation_id?: string; status?: string } = {},
) {
  return client.rpc<BotRun[]>("bot.run.list", filters);
}
export function botRunCancel(
  runId: string,
  expectedVersion: number,
  idempotencyKey: string,
  reason?: string,
) {
  return client.rpc<BotRun>("bot.run.cancel", {
    run_id: runId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
    ...(reason ? { reason } : {}),
  });
}
export function botRunInput(
  runId: string,
  requestId: string,
  input: unknown[],
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotRun>("bot.run.input", {
    run_id: runId,
    request_id: requestId,
    input,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botRunApproval(
  runId: string,
  approvalId: string,
  allow: boolean,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotRun>("bot.run.approval", {
    run_id: runId,
    approval_id: approvalId,
    allow,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
