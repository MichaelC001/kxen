import type { ModelInfo, ProviderCatalog } from "../../lib/models";

export const ROLE_ASSIGN: Array<{ role: string; label: string }> = [
  { role: "chat", label: "设为主会话模型" },
  { role: "thinking", label: "设为思考模型" },
  { role: "planning", label: "设为规划模型" },
  { role: "execution", label: "设为执行模型" },
  { role: "review", label: "设为审查模型" },
  { role: "research", label: "设为调研模型" },
];

export interface ModelPickerRow {
  provider: string;
  providerName: string;
  model: ModelInfo;
}

export function modelRows(catalog: ProviderCatalog[]): ModelPickerRow[] {
  return catalog.flatMap((provider) =>
    provider.models.map((model) => ({
      provider: provider.provider,
      providerName: provider.provider_name,
      model,
    })),
  );
}

export function filterModelRows(rows: ModelPickerRow[], query: string): ModelPickerRow[] {
  const normalized = query.toLowerCase();
  if (!normalized) return rows;
  return rows.filter((row) =>
    `${row.providerName} ${row.model.name} ${row.model.id}`.toLowerCase().includes(normalized),
  );
}
