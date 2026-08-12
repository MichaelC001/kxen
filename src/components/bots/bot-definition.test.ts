import { describe, expect, it } from "vitest";
import type { BotDefinition, BotState } from "../../lib/bots";
import { encodeBotInput, publishedBotDefinition } from "./bot-definition";

const jsonDefinition = {
  input_contract: { content_type: "application/json", required_fields: ["topic"] },
} as BotDefinition;

describe("Bot input contracts", () => {
  it("encodes structured input and rejects missing required fields", () => {
    expect(encodeBotInput('{"topic":"status","limit":2}', jsonDefinition)).toEqual([
      {
        kind: "data",
        schema_id: "bot_contract_input",
        fields: { topic: "status", limit: "2" },
      },
    ]);
    expect(() => encodeBotInput('{"limit":2}', jsonDefinition)).toThrow("缺少必填字段：topic");
  });

  it("resolves the exact published revision instead of an unpublished draft", () => {
    const published = { display_name: "Published" } as BotDefinition;
    const state = {
      current_revision_id: "brev_published",
      revisions: {
        one: { revision_id: "brev_published", definition: published },
      },
      draft: { definition: { display_name: "Draft" } },
    } as unknown as BotState;
    expect(publishedBotDefinition(state)).toBe(published);
  });
});
