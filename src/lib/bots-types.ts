export type BotLifecycle = "draft" | "active" | "paused" | "archived" | "trashed" | "blocked";

export interface BotSummary {
  bot_id: string;
  display_name: string;
  lifecycle: BotLifecycle;
  current_revision_id?: string;
  current_revision_number?: number;
  draft_version?: number;
  blocked_reason?: string;
  updated_at_ms: number;
}

export interface BotState {
  bot_id: string;
  lifecycle: BotLifecycle;
  event_version: number;
  draft_version_counter: number;
  draft?: { version: number; content_hash: string; definition: BotDefinition };
  current_revision_id?: string;
  revisions: Record<
    string,
    {
      revision_id: string;
      revision_number: number;
      content_hash: string;
      definition: BotDefinition;
    }
  >;
  blocked_reason?: string;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface BotDefinition {
  display_name: string;
  description: string;
  objective: string;
  success_criteria: string[];
  instructions: string;
  input_contract: { description: string; content_type: string; required_fields: string[] };
  output_contract: { description: string; content_type: string; required_fields: string[] };
  mrm_role: string;
  capabilities: string[];
  resources: { workspaces: unknown[]; connectors: string[] };
  approval: string;
  budget: Record<string, number | null>;
  context: Record<string, number>;
  memory: { enabled: boolean; max_items: number; allow_sensitive: boolean };
  communication: { allow_direct: boolean; allow_groups: boolean; allowed_peers: string[] };
  failure: { max_pure_retries: number; auto_pause_after_failures: number };
}

export interface BuilderState {
  builder_session_id: string;
  bot_id: string;
  lifecycle: string;
  event_version: number;
  user_goal: string;
  draft?: {
    version: number;
    source_message_id?: string;
    content_hash: string;
    definition: BotDefinition;
  };
  grants: Array<{ grant_id: string; draft_hash: string; permission_hash: string; reason: string }>;
  reports: Array<{
    report_id: string;
    draft_hash: string;
    publish_eligible: boolean;
    findings: Array<{
      code: string;
      status: "PASS" | "FAIL" | "UNKNOWN";
      message: string;
      evidence?: string;
    }>;
  }>;
  tests: Array<{ run_id: string; draft_hash: string; passed: boolean; summary: string }>;
  active_test_run_id?: string;
  blocked_reason?: string;
}

export interface BotRun {
  spec: {
    run_id: string;
    bot_id: string;
    revision_id: string;
    conversation_id?: string;
    task_id?: string;
    trigger: { kind: string };
  };
  status: string;
  event_version: number;
  result: Array<{ kind: string; text?: string }>;
  approval?: { approval_id: string; operation_id?: string; summary: string };
  input_request?: { request_id: string; prompt: string };
  artifacts: Array<{
    artifact_id: string;
    display_name: string;
    media_type: string;
    content_hash: string;
    size_bytes: number;
  }>;
  error_code?: string;
  error_message?: string;
  cancellation_requested?: string;
  usage: {
    input_tokens: number;
    output_tokens: number;
    tool_calls: number;
    turns: number;
    wall_clock_ms: number;
  };
  updated_at_ms: number;
}

export interface BotConversation {
  conversation_id: string;
  kind: "human_bot" | "bot_direct" | "bot_group";
  lifecycle: string;
  event_version: number;
  moderator_bot_id?: string;
  members: Record<string, { bot_id: string; active: boolean }>;
  messages: Array<{
    message_id: string;
    kind: string;
    actor: BotActor;
    parts: BotMessagePart[];
    target_bot_id?: string;
    created_at_ms: number;
  }>;
  message_sequences: Record<string, number>;
  tasks: Record<string, BotTask>;
}

export type BotActor =
  | { kind: "owner" }
  | { kind: "bot"; id: string }
  | { kind: "agent"; id: string; scope: { kind: string; id: string } }
  | { kind: "system"; actor: string };

export type BotMessagePart =
  | { kind: "text"; text: string }
  | { kind: "data"; schema_id: string; fields: Record<string, string> }
  | {
      kind: "artifact_ref";
      artifact: {
        artifact_id: string;
        display_name: string;
        media_type: string;
        content_hash: string;
        size_bytes: number;
      };
    };

export interface BotTask {
  task_id: string;
  conversation_id: string;
  title: string;
  owner_bot_id: string;
  status: string;
  event_version?: number;
}

export interface BotRoutine {
  routine_id: string;
  lifecycle: string;
  event_version: number;
  definition: RoutineDefinition;
  next_scheduled_at_ms?: number;
  consecutive_failures: number;
  blocked_reason?: string;
  occurrences: Record<
    string,
    {
      occurrence_id: string;
      status: string;
      manual: boolean;
      run_id?: string;
      error?: string;
      observed_at_ms: number;
    }
  >;
}

export interface RoutineDefinition {
  bot_id: string;
  name: string;
  schedule: {
    expression: { kind: "cron"; expression: string } | { kind: "once"; at_ms: number };
    timezone: string;
    misfire: "skip" | "run_once";
    max_lateness_ms: number;
  };
  context_mode: "isolated" | "continue_conversation";
  target_conversation_id?: string;
  input: Array<
    | { kind: "text"; text: string }
    | { kind: "data"; schema_id: string; fields: Record<string, string> }
  >;
  budget_override?: Record<string, number | null>;
  revision_policy: { kind: "follow_current" } | { kind: "pinned"; revision_id: string };
  failure_threshold: number;
}

export interface BotMemoryState {
  event_version: number;
  items: Record<
    string,
    { item_id: string; kind: string; content: string; version: number; updated_at_ms: number }
  >;
}

export interface BotRecoverySnapshot {
  registry: Array<{
    recovery_id: string;
    aggregate: { kind: string; id: string };
    reason: string;
    evidence: string[];
    opened_at_ms: number;
  }>;
  bots: BotState[];
  runs: BotRun[];
  conversations: BotConversation[];
  routines: BotRoutine[];
}

export interface BotPostOptions {
  mentions?: string[];
  everyone?: boolean;
  reply_to_message_id?: string;
  task_id?: string;
  correlation_id?: string;
  task?: {
    task_id: string;
    owner_bot_id: string;
    title: string;
    input: BotMessagePart[];
    expected_output: string;
    parent_task_id?: string;
    budget: Record<string, number | null>;
  };
}

export const newBotId = (prefix: string) => `${prefix}_${crypto.randomUUID().replaceAll("-", "")}`;
