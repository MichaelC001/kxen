export const definition = {
  display_name: "Report Bot",
  description: "Build reports",
  objective: "Create a report",
  success_criteria: ["accurate", "complete"],
  instructions: "Use evidence",
  input_contract: { description: "request", content_type: "text/plain", required_fields: [] },
  output_contract: { description: "report", content_type: "text/plain", required_fields: [] },
  mrm_role: "execution",
  capabilities: ["read", "bot_artifact"],
  resources: {
    workspaces: [
      { workspace_id: "workspace_project", paths: [{ relative_path: "reports", access: "write" }] },
    ],
    connectors: ["github"],
  },
  approval: "ask",
  budget: { max_turns: 4 },
  context: { max_parts: 20 },
  memory: { enabled: true, max_items: 20, allow_sensitive: false },
  communication: { allow_direct: true, allow_groups: true, allowed_peers: ["bot_peer"] },
  failure: { max_pure_retries: 1, auto_pause_after_failures: 3 },
};

export const state = {
  bot_id: "bot_report",
  lifecycle: "active",
  event_version: 7,
  draft_version_counter: 1,
  draft: { version: 1, content_hash: "draft_hash", definition },
  current_revision_id: "revision_report_1",
  revisions: {
    revision_report_1: {
      revision_id: "revision_report_1",
      revision_number: 1,
      content_hash: "draft_hash",
      definition,
    },
  },
  created_at_ms: 1,
  updated_at_ms: 2,
};

export const builder = {
  builder_session_id: "builder_report",
  bot_id: "bot_report",
  lifecycle: "active",
  event_version: 8,
  user_goal: "Create reports",
  messages: [
    {
      message_id: "message_1",
      actor: { kind: "owner" },
      text: "Create reports",
      created_at_ms: 1,
    },
    {
      message_id: "message_2",
      actor: { kind: "bot", id: "bot_report" },
      text: "I created a Report Bot draft and kept its requested identity.",
      created_at_ms: 2,
    },
  ],
  draft: { version: 1, source_message_id: "message_1", content_hash: "draft_hash", definition },
  grants: [
    {
      grant_id: "grant_1",
      draft_hash: "draft_hash",
      permission_hash: "permission_1",
      reason: "reviewed",
    },
  ],
  reports: [
    {
      report_id: "report_1",
      draft_hash: "draft_hash",
      publish_eligible: true,
      findings: [{ code: "contract", status: "PASS", message: "valid", evidence: "test" }],
    },
  ],
  tests: [{ run_id: "run_test", draft_hash: "draft_hash", passed: true, summary: "PASS" }],
};
