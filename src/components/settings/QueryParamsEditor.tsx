// 自定义提供商的 per-request query 参数编辑（Azure OpenAI 的 api-version 等）：
// 键值对行可增删；键值规则与后端 custom_provider 校验一致，提交/探测时由 collectQueryParams 收口。
import { For } from "solid-js";
import { Trash2 } from "lucide-solid";
import { queryParamRows, setQueryParamRows } from "./add-account-form";

export default function QueryParamsEditor() {
  const update = (index: number, patch: Partial<{ key: string; value: string }>) =>
    setQueryParamRows((prev) => prev.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  const remove = (index: number) => setQueryParamRows((prev) => prev.filter((_, i) => i !== index));

  return (
    <div class="space-y-1.5">
      <div class="flex items-center justify-between">
        <span class="text-2xs text-[var(--text-faint)]">
          query 参数（可选；Azure OpenAI 需 api-version）
        </span>
        <button
          class="pressable px-2 py-0.5 rounded text-2xs border border-[var(--border)]"
          onClick={() => setQueryParamRows((prev) => [...prev, { key: "", value: "" }])}
        >
          添加参数
        </button>
      </div>
      <For each={queryParamRows()}>
        {(row, index) => (
          <div class="flex items-center gap-2">
            <input
              class="form-mono flex-1"
              placeholder="键（如 api-version）"
              value={row.key}
              onInput={(e) => update(index(), { key: e.currentTarget.value })}
            />
            <input
              class="form-mono flex-1"
              placeholder="值（如 2025-01-01-preview）"
              value={row.value}
              onInput={(e) => update(index(), { value: e.currentTarget.value })}
            />
            <button
              class="pressable px-1.5 py-1 rounded text-[var(--text-faint)] hover:text-[var(--err)]"
              title="删除该参数"
              onClick={() => remove(index())}
            >
              <Trash2 size={12} />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
