// 调度实况台：provider 槽位 / 角色绑定与降级链 / 试派发验证 / 最近派发历史。
// provider 下拉与默认模型来自后端 provider.list（registry），前端不维护硬编码清单。
import { createSignal, For, onMount, Show } from "solid-js";
import { Play } from "lucide-solid";
import { configGet, configSetRole, type RoleBindingView } from "../../lib/chat";
import {
  mrmStats,
  providerAccounts,
  providerList,
  testDispatch,
  type AccountInfo,
  type DispatchRecord,
  type ProviderInfo,
  type TestDispatchResult,
} from "../../lib/provider";
import { fmtCtx, modelsCatalog, type ProviderCatalog } from "../../lib/models";

const ROLE_LABELS: Record<string, string> = {
  chat: "主会话",
  thinking: "思考分析",
  planning: "任务规划",
  execution: "高速执行",
  review: "审查验证",
  research: "调研搜索",
};

interface Slot {
  provider: string;
  available: number;
  limit: number;
}

function parseSlots(describe: string): Slot[] {
  const out: Slot[] = [];
  for (const line of describe.split("\n")) {
    const m = line.match(/^(\S+):\s*(\d+)\/(\d+) available$/);
    if (m) out.push({ provider: m[1]!, available: Number(m[2]), limit: Number(m[3]) });
  }
  return out;
}

export default function RoutingSection() {
  const [roles, setRoles] = createSignal<Record<string, RoleBindingView>>({});
  const [slots, setSlots] = createSignal<Slot[]>([]);
  const [history, setHistory] = createSignal<DispatchRecord[]>([]);
  const [accounts, setAccounts] = createSignal<AccountInfo[]>([]);
  const [providers, setProviders] = createSignal<ProviderInfo[]>([]);
  const [cat, setCat] = createSignal<ProviderCatalog[]>([]);
  const [testing, setTesting] = createSignal("");
  const [testResult, setTestResult] = createSignal<Record<string, TestDispatchResult>>({});
  const [saved, setSaved] = createSignal("");

  const reload = async () => {
    const [cfg, stats, accs, catalog, list] = await Promise.all([
      configGet().catch(() => null),
      mrmStats().catch(() => null),
      providerAccounts().catch(() => []),
      modelsCatalog().catch(() => []),
      providerList().catch(() => [] as ProviderInfo[]),
    ]);
    if (cfg?.roles) setRoles(cfg.roles);
    if (stats) {
      setSlots(parseSlots(stats.describe));
      setHistory(stats.history.slice(0, 10));
    }
    setAccounts(accs);
    setCat(catalog);
    setProviders(list);
  };
  onMount(() => void reload());

  const flash = (msg: string) => {
    setSaved(msg);
    setTimeout(() => setSaved(""), 2000);
  };

  const accountOptions = (provider: string) => accounts().filter((a) => a.provider === provider);

  // P0-4：update 永远以当前 binding 全量合并后下发（后端缺省会沿用旧值，见 settings.rs merge_binding）。
  // provider 变更时仅清 account（账号归属 provider，留旧账号会绑错）；fallback 是角色间降级关系，
  // 与 provider 无关，必须保留。想清除的字段显式传 ""（None/缺省在后端是「沿用旧值」语义）。
  const update = async (role: string, patch: Partial<RoleBindingView>) => {
    const cur = roles()[role] ?? {
      provider: "anthropic",
      model: "",
      account: null,
      fallback: null,
    };
    const next = { ...cur, ...patch };
    if (patch.provider !== undefined && patch.provider !== cur.provider) next.account = null;
    await configSetRole(role, next.provider, next.model, next.fallback ?? "", next.account ?? "");
    setRoles((prev) => ({
      ...prev,
      [role]: {
        provider: next.provider,
        model: next.model,
        account: next.account ?? null,
        fallback: next.fallback ?? null,
      },
    }));
    flash(`${ROLE_LABELS[role] ?? role} 已保存并热生效`);
  };

  const tryDispatch = async (role: string) => {
    setTesting(role);
    try {
      const r = await testDispatch(role);
      setTestResult((prev) => ({ ...prev, [role]: r }));
      await reload();
    } finally {
      setTesting("");
    }
  };

  const defaultModelOf = (provider: string) =>
    providers().find((p) => p.key === provider)?.default_model ?? "";

  return (
    <>
      <div class="text-xs text-[var(--text-faint)]">
        调度实况（MRM 全局路由；槽位为空自动降级到 fallback 角色）
      </div>
      <Show when={saved()}>
        <div class="text-xs text-[var(--ok)]">{saved()}</div>
      </Show>

      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] px-4 py-3">
        <div class="text-xs text-[var(--text-faint)] mb-2">并发槽位</div>
        <div class="space-y-1.5">
          <For
            each={slots()}
            fallback={<div class="text-xs text-[var(--text-faint)]">无运行中派发</div>}
          >
            {(s) => (
              <div class="flex items-center gap-2 text-xs">
                <span class="w-24 text-[var(--text-dim)]">{s.provider}</span>
                <span class="ctx-bar flex-1">
                  <span class="ctx-bar-fill" style={`width:${(s.available / s.limit) * 100}%`} />
                </span>
                <span class="tabular-nums text-[var(--text-faint)]">
                  {s.available}/{s.limit}
                </span>
              </div>
            )}
          </For>
        </div>
      </div>

      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
        <For each={Object.keys(ROLE_LABELS)}>
          {(role) => {
            const binding = () => roles()[role] ?? { provider: "anthropic", model: "" };
            const result = () => testResult()[role];
            return (
              <div class="px-4 py-3">
                <div class="flex items-center gap-3">
                  <div class="w-20 shrink-0">
                    <div class="text-sm">{ROLE_LABELS[role]}</div>
                    <div class="text-2xs text-[var(--text-faint)]">{role}</div>
                  </div>
                  <select
                    class="bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-xs text-[var(--text-dim)]"
                    value={binding().provider}
                    onChange={(e) => {
                      const provider = e.currentTarget.value;
                      void update(role, {
                        provider,
                        model: binding().model || defaultModelOf(provider),
                      });
                    }}
                  >
                    <For each={providers()}>
                      {(p) => <option value={p.key}>{p.display}</option>}
                    </For>
                  </select>
                  <select
                    class="bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-xs text-[var(--text-dim)]"
                    title="账号：轮转 = 槽满自动换下一个账号"
                    value={binding().account ?? ""}
                    onChange={(e) => void update(role, { account: e.currentTarget.value || null })}
                  >
                    <option value="">账号轮转</option>
                    <For each={accountOptions(binding().provider)}>
                      {(a) => <option value={a.account}>{a.account}</option>}
                    </For>
                  </select>
                  <input
                    list={`models-${role}`}
                    class="flex-1 bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs font-mono"
                    value={binding().model}
                    placeholder="model id（可下拉搜索）"
                    onChange={(e) => void update(role, { model: e.currentTarget.value })}
                  />
                  <datalist id={`models-${role}`}>
                    <For each={cat().find((p) => p.provider === binding().provider)?.models ?? []}>
                      {(m) => (
                        <option value={m.id}>{`${m.name} · ctx ${fmtCtx(m.context)}`}</option>
                      )}
                    </For>
                  </datalist>
                  <select
                    class="bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-xs text-[var(--text-dim)]"
                    title="降级目标角色：本角色槽位满时降级到该角色"
                    value={binding().fallback ?? ""}
                    onChange={(e) => void update(role, { fallback: e.currentTarget.value || null })}
                  >
                    <option value="">无降级</option>
                    <For each={Object.keys(ROLE_LABELS).filter((r) => r !== role)}>
                      {(r) => <option value={r}>{ROLE_LABELS[r]}</option>}
                    </For>
                  </select>
                  <Show when={binding().fallback}>
                    <span class="text-2xs text-[var(--text-faint)]" title="降级目标角色">
                      → {binding().fallback}
                    </span>
                  </Show>
                  <button
                    class="pressable flex items-center gap-1 px-2 py-1 rounded text-2xs border border-[var(--border)]"
                    disabled={testing() === role}
                    onClick={() => void tryDispatch(role)}
                    title="真实派发一个 PONG 子代理验证路由"
                  >
                    <Play size={10} />
                    {testing() === role ? "派发中" : "试派发"}
                  </button>
                </div>
                <Show when={result()}>
                  {(r) => (
                    <div class="mt-1.5 text-2xs text-[var(--text-faint)]">
                      实测路由：{r().provider}/{r().model}
                      <Show when={r().account}>（账号 {r().account}）</Show>
                      <Show when={r().degraded_from}>（降级自 {r().degraded_from}）</Show> · 应答：
                      {r().answer}
                    </div>
                  )}
                </Show>
              </div>
            );
          }}
        </For>
      </div>

      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)]">
        <div class="px-4 py-2 border-b border-[var(--border)] text-xs text-[var(--text-faint)]">
          最近派发
        </div>
        <div class="divide-y divide-[var(--border)]">
          <For
            each={history()}
            fallback={<div class="px-4 py-3 text-xs text-[var(--text-faint)]">暂无派发记录</div>}
          >
            {(h) => (
              <div class="px-4 py-2 flex items-center gap-3 text-xs">
                <span class="w-20 text-[var(--text-dim)]">{h.role}</span>
                <span class="font-mono flex-1 truncate">
                  {h.provider}/{h.model}
                </span>
                <Show when={h.degraded_from}>
                  <span class="text-2xs text-[var(--warn)]">降级</span>
                </Show>
                <span class="text-2xs text-[var(--text-faint)] tabular-nums">
                  {new Date(h.at).toLocaleTimeString("zh-CN", { hour12: false })}
                </span>
              </div>
            )}
          </For>
        </div>
      </div>
    </>
  );
}
