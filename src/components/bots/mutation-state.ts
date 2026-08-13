import type { BotConversation, BotRoutine, BotRun } from "../../lib/bots";

export function findDirectConversation(
  conversations: BotConversation[],
  leftBotId: string,
  rightBotId: string,
): BotConversation | undefined {
  const expected = [leftBotId, rightBotId].sort().join(",");
  return conversations.find(
    (conversation) =>
      conversation.kind === "bot_direct" &&
      conversation.lifecycle !== "archived" &&
      Object.values(conversation.members)
        .filter((member) => member.active)
        .map((member) => member.bot_id)
        .sort()
        .join(",") === expected,
  );
}

export function conversationLifecycleAfter(kind: "pause" | "resume" | "archive" | "stop"): string {
  return { pause: "paused", resume: "active", archive: "archived", stop: "paused" }[kind];
}

export function routineDefinitionApplied(
  routines: BotRoutine[],
  routineId: string,
  definitionKey: string,
): boolean {
  return (
    JSON.stringify(routines.find((routine) => routine.routine_id === routineId)?.definition) ===
    definitionKey
  );
}

export function editableRoutineCron(routine: BotRoutine): string {
  const expression = routine.definition.schedule.expression;
  return expression.kind === "cron" ? expression.expression : "0 9 * * *";
}

export function editableRoutineInput(routine: BotRoutine): string {
  return routine.definition.input
    .map((part) => (part.kind === "text" ? part.text : JSON.stringify(part.fields, null, 2)))
    .join("\n");
}

export function routineMutationApplied(
  routines: BotRoutine[],
  routineId: string,
  kind: "pause" | "resume" | "run" | "trash",
  occurrenceId: string,
): boolean {
  const current = routines.find((routine) => routine.routine_id === routineId);
  if (kind === "run") return Boolean(current?.occurrences[occurrenceId]);
  return current?.lifecycle === { pause: "paused", resume: "active", trash: "trashed" }[kind];
}

export function runCancellationApplied(run: BotRun | null, runId: string): boolean {
  return Boolean(
    run?.spec.run_id === runId && (run.cancellation_requested || isTerminalRun(run.status)),
  );
}

function isTerminalRun(status: string): boolean {
  return ["completed", "failed", "canceled", "rejected", "blocked"].includes(status);
}
