import type { KnowledgeEntry, KnowledgeScope } from "../../lib/knowledge";

export const SCOPES: { id: KnowledgeScope; label: string; hint: string }[] = [
  { id: "project", label: "项目", hint: ".agents/，入 git 共享" },
  { id: "personal", label: "个人", hint: "~/.agents/，跨项目" },
];

export const TYPE_LABELS: Record<string, string> = {
  rule: "规则",
  reference: "参考",
  skill: "技能",
  command: "命令",
  note: "笔记",
  memory: "记忆",
  history: "历史",
};

const TYPE_ORDER = ["rule", "note", "memory", "reference", "skill", "command", "history"];

export const NOTE_TYPES = ["correction", "convention", "pitfall", "preference", "note"];

export function typesFor(entries: KnowledgeEntry[], scope: KnowledgeScope): string[] {
  const types = [
    ...new Set(entries.filter((entry) => entry.scope === scope).map((entry) => entry.type)),
  ];
  return types.sort((left, right) => {
    const leftIndex = TYPE_ORDER.indexOf(left);
    const rightIndex = TYPE_ORDER.indexOf(right);
    if (leftIndex >= 0 || rightIndex >= 0) {
      return (
        (leftIndex < 0 ? Number.MAX_SAFE_INTEGER : leftIndex) -
        (rightIndex < 0 ? Number.MAX_SAFE_INTEGER : rightIndex)
      );
    }
    return left.localeCompare(right);
  });
}

export function shadowedConceptIds(entries: KnowledgeEntry[]): Set<string> {
  const projectIds = new Set(
    entries.filter((entry) => entry.scope === "project").map((entry) => entry.concept_id),
  );
  return new Set(
    entries
      .filter((entry) => entry.scope === "personal" && projectIds.has(entry.concept_id))
      .map((entry) => entry.concept_id),
  );
}
