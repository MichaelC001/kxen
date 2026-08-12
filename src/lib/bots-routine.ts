import { client } from "./client";
import type { BotRoutine, RoutineDefinition } from "./bots-types";

export function botRoutineList(botId?: string) {
  return client.rpc<BotRoutine[]>("bot.routine.list", botId ? { bot_id: botId } : {});
}
export function botRoutineCreate(
  routineId: string,
  definition: RoutineDefinition,
  idempotencyKey: string,
) {
  return client.rpc<BotRoutine>("bot.routine.create", {
    routine_id: routineId,
    definition,
    idempotency_key: idempotencyKey,
  });
}
export function botRoutineUpdate(
  routineId: string,
  definition: RoutineDefinition,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotRoutine>("bot.routine.update", {
    routine_id: routineId,
    definition,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botRoutinePause(
  routineId: string,
  expectedVersion: number,
  idempotencyKey: string,
  reason?: string,
) {
  return client.rpc<BotRoutine>("bot.routine.pause", {
    routine_id: routineId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
    ...(reason ? { reason } : {}),
  });
}
export function botRoutineResume(
  routineId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotRoutine>("bot.routine.resume", {
    routine_id: routineId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botRoutineRunNow(
  routineId: string,
  occurrenceId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotRoutine>("bot.routine.run_now", {
    routine_id: routineId,
    occurrence_id: occurrenceId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botRoutineTrash(
  routineId: string,
  expectedVersion: number,
  idempotencyKey: string,
) {
  return client.rpc<BotRoutine>("bot.routine.trash", {
    routine_id: routineId,
    expected_version: expectedVersion,
    idempotency_key: idempotencyKey,
  });
}
export function botRoutineHistory(routineId: string) {
  return client.rpc("bot.routine.history", { routine_id: routineId });
}
