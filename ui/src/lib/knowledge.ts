import { client } from "./client";

export interface KnowledgeEntry {
  scope: string;
  slug: string;
  type: string;
  description: string;
  date: string;
  content: string;
  path: string;
  enabled: boolean;
}

export function knowledgeList(): Promise<KnowledgeEntry[]> {
  return client.rpc("knowledge.list");
}

export function knowledgeAdd(
  scope: string,
  type: string,
  description: string,
  content: string,
): Promise<void> {
  return client.rpc("knowledge.add", { scope, type, description, content });
}

export function knowledgeRemove(scope: string, slug: string): Promise<void> {
  return client.rpc("knowledge.remove", { scope, slug });
}

export function knowledgeSetEnabled(scope: string, slug: string, enabled: boolean): Promise<void> {
  return client.rpc("knowledge.set_enabled", { scope, slug, enabled });
}

export function knowledgeMove(scope: string, slug: string, to: string): Promise<void> {
  return client.rpc("knowledge.move", { scope, slug, to });
}

export interface InjectionPreview {
  project: string | null;
  extra: string | null;
}

export function knowledgeInjectionPreview(): Promise<InjectionPreview> {
  return client.rpc("knowledge.injection_preview");
}
