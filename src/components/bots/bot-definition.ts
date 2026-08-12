import type { BotDefinition, BotState, RoutineDefinition } from "../../lib/bots";

export function publishedBotDefinition(state: BotState | null): BotDefinition | undefined {
  if (!state?.current_revision_id) return undefined;
  return Object.values(state.revisions).find(
    (revision) => revision.revision_id === state.current_revision_id,
  )?.definition;
}

export function editableBotDefinition(state: BotState): BotDefinition | undefined {
  return state.draft?.definition ?? publishedBotDefinition(state);
}

export function encodeBotInput(
  text: string,
  definition: BotDefinition | undefined,
): RoutineDefinition["input"] {
  if (definition?.input_contract.content_type !== "application/json") {
    return [{ kind: "text", text }];
  }
  const parsed = JSON.parse(text) as unknown;
  if (!parsed || Array.isArray(parsed) || typeof parsed !== "object") {
    throw new Error("输入必须是 JSON object");
  }
  const fields = Object.fromEntries(
    Object.entries(parsed).map(([key, value]) => [
      key,
      typeof value === "string" ? value : JSON.stringify(value),
    ]),
  );
  const missing = definition.input_contract.required_fields.filter((field) => !(field in fields));
  if (missing.length > 0) {
    throw new Error(`缺少必填字段：${missing.join(", ")}`);
  }
  return [{ kind: "data", schema_id: "bot_contract_input", fields }];
}
