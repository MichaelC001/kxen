import { client } from "./client";

export type KnowledgeScope = "project" | "personal";
export type KnowledgeKind =
  | "rule"
  | "reference"
  | "skill"
  | "command"
  | "note"
  | "memory"
  | "history"
  | "generic";

export interface KnowledgeEntry {
  scope: KnowledgeScope;
  type: string;
  kind: KnowledgeKind;
  concept_id: string;
  slug: string;
  title: string;
  description: string;
  content: string;
  path: string;
  resource?: string;
  tags: string[];
  status?: string;
  stale_after?: string;
  links: string[];
  okf_conformant: boolean;
  reserved?: "index" | "log";
  okf_version?: string;
  enabled: boolean;
  always_apply: boolean;
  globs: string[];
  needs: string[];
  when_to_use?: string;
  argument_hint?: string;
  note_type?: string;
  date: string;
}

export function knowledgeList(): Promise<KnowledgeEntry[]> {
  return client.rpc("knowledge.list");
}

export function knowledgeAdd(
  scope: KnowledgeScope,
  type: string,
  description: string,
  content: string,
): Promise<void> {
  return client.rpc("knowledge.add", { scope, type, description, content });
}

export function knowledgeRemove(scope: KnowledgeScope, conceptId: string): Promise<void> {
  return client.rpc("knowledge.remove", { scope, slug: conceptId });
}

export function knowledgeSetEnabled(
  scope: KnowledgeScope,
  conceptId: string,
  enabled: boolean,
): Promise<void> {
  return client.rpc("knowledge.set_enabled", { scope, slug: conceptId, enabled });
}

export function knowledgeMove(
  scope: KnowledgeScope,
  conceptId: string,
  to: KnowledgeScope,
): Promise<void> {
  return client.rpc("knowledge.move", { scope, slug: conceptId, to });
}

export interface InjectionPreview {
  block: string | null;
}

export function knowledgeInjectionPreview(sessionId?: string): Promise<InjectionPreview> {
  return client.rpc("knowledge.injection_preview", sessionId ? { session_id: sessionId } : {});
}

export interface BlockedConsolidationAttempt {
  session_id: string;
  status: "provider_result_unknown";
  reason: string;
  message_revision: number | null;
  usage_unknown: boolean;
  metering_settled: boolean;
}

export interface AcknowledgeUnknownResult {
  session_id: string;
  checkpointed_revision: number | null;
  usage_unknown_recorded: boolean;
  diagnostics: string[];
}

export function knowledgeConsolidationBlocked(): Promise<BlockedConsolidationAttempt[]> {
  return client.rpc("knowledge.consolidation_blocked");
}

export function knowledgeAcknowledgeUnknown(sessionId: string): Promise<AcknowledgeUnknownResult> {
  return client.rpc("knowledge.consolidation_acknowledge_unknown", {
    session_id: sessionId,
    confirm_unknown: true,
  });
}

export interface CodingRulesInfo {
  enabled: boolean;
  content: string;
}

export function codingRulesGet(): Promise<CodingRulesInfo> {
  return client.rpc("coding_rules.get");
}

export function codingRulesSet(enabled: boolean): Promise<void> {
  return client.rpc("coding_rules.set", { enabled });
}
