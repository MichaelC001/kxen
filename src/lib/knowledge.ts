import { client } from "./client";

export type KnowledgeScope = "project" | "personal";
export type KnowledgeKind =
  | "rule"
  | "reference"
  | "skill"
  | "command"
  | "note"
  | "memory"
  | "history";

export interface KnowledgeEntry {
  scope: KnowledgeScope;
  kind: KnowledgeKind;
  slug: string;
  description: string;
  content: string;
  path: string;
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

export function knowledgeRemove(scope: KnowledgeScope, slug: string): Promise<void> {
  return client.rpc("knowledge.remove", { scope, slug });
}

export function knowledgeSetEnabled(
  scope: KnowledgeScope,
  slug: string,
  enabled: boolean,
): Promise<void> {
  return client.rpc("knowledge.set_enabled", { scope, slug, enabled });
}

export function knowledgeMove(
  scope: KnowledgeScope,
  slug: string,
  to: KnowledgeScope,
): Promise<void> {
  return client.rpc("knowledge.move", { scope, slug, to });
}

export interface InjectionPreview {
  block: string | null;
}

export function knowledgeInjectionPreview(sessionId?: string): Promise<InjectionPreview> {
  return client.rpc("knowledge.injection_preview", sessionId ? { session_id: sessionId } : {});
}
