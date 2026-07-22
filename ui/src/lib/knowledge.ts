import { client } from "./client";

export interface KnowledgeEntry {
  scope: string;
  slug: string;
  type: string;
  description: string;
  date: string;
  content: string;
  path: string;
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
