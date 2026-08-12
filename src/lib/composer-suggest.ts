import { client } from "./client";

export interface ComposerSuggestion {
  id: string;
  kind: "file" | "insert_text";
  path: string;
  label: string;
  reason: string;
  source: "local" | "semantic" | "llm";
  score: number;
}

export interface ComposerSuggestResponse {
  suggestions: ComposerSuggestion[];
  trusted: boolean;
}

export interface ComposerRemoteSuggestResponse {
  suggestions: ComposerSuggestion[];
  warnings: string[];
}

export async function composerSuggestLocal(
  draft: string,
  sessionId: string,
  selectedPaths: string[],
  limit = 6,
): Promise<ComposerSuggestResponse> {
  return client.rpc("composer.suggest.local", {
    draft,
    session_id: sessionId || undefined,
    selected_paths: selectedPaths,
    limit,
  });
}

export async function composerSuggestRemote(
  draft: string,
  sessionId: string,
  selectedPaths: string[],
  candidateIds: string[],
  requestId: string,
  limit = 6,
): Promise<ComposerRemoteSuggestResponse> {
  return client.rpc("composer.suggest.remote", {
    draft,
    session_id: sessionId,
    selected_paths: selectedPaths,
    candidate_ids: candidateIds,
    request_id: requestId,
    limit,
  });
}

export async function composerSuggestCancel(sessionId: string, requestId?: string): Promise<void> {
  return client.rpc("composer.suggest.cancel", {
    session_id: sessionId,
    request_id: requestId,
  });
}

export async function configSetComposerSuggestions(
  key: "enabled" | "semantic" | "llm",
  enabled: boolean,
): Promise<void> {
  return client.rpc("config.set_composer_suggestions", { key, enabled });
}

export async function configSetEmbedding(
  provider: string,
  model: string,
  baseUrl: string,
): Promise<void> {
  return client.rpc("config.set_embedding", { provider, model, base_url: baseUrl });
}
