import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import KnowledgeSection from "../components/settings/KnowledgeSection";
import DoctorSection from "../components/settings/DoctorSection";
import McpSection from "../components/settings/McpSection";
import ProvidersSection from "../components/settings/ProvidersSection";
import RoutingSection from "../components/settings/RoutingSection";
import ScheduleSection from "../components/settings/ScheduleSection";
import UsageSection from "../components/settings/UsageSection";
import VoiceSection from "../components/settings/VoiceSection";
import GeneralSection from "../components/settings/GeneralSection";
import ApprovalRulesSection from "../components/settings/ApprovalRulesSection";
import ApprovalHistorySection from "../components/settings/ApprovalHistorySection";
import ComposerSuggestionsSection from "../components/settings/ComposerSuggestionsSection";
import { client } from "../lib/client";
import { configGet } from "../lib/chat";
import { mrmReadiness, type RouteReadinessStatus } from "../lib/provider";
import { flashErr, flashOk } from "../lib/flash";
import { formatError } from "../lib/error-text";
import { onDragStart } from "../lib/drag";
import { isTauri, isWeb } from "../lib/runtime";
import { onTabKeyDown } from "../lib/tabs";

const SECTIONS = [
  "通用",
  "提供商",
  "语音",
  "模型路由",
  "用量与统计",
  "知识库 OKF",
  "定时任务",
  "诊断",
  "高级",
] as const;

export default function Settings() {
  const [section, setSection] = createSignal<(typeof SECTIONS)[number]>("通用");
  const [sendPolicy, setSendPolicy] = createSignal("queue");
  const [experimental, setExperimental] = createSignal({
    automatic_knowledge_distillation: false,
    browser_automation: false,
    remote_mcp: false,
  });
  const [readiness, setReadiness] = createSignal({
    workspace: null as boolean | null,
    chat: null as boolean | null,
    agents: null as boolean | null,
    chatDetail: "",
    agentsDetail: "",
  });
  const [configLoaded, setConfigLoaded] = createSignal(false);
  const [configErr, setConfigErr] = createSignal("");
  const [policySaving, setPolicySaving] = createSignal(false);
  const [experimentalSaving, setExperimentalSaving] = createSignal<ReadonlySet<string>>(new Set());

  const applyConfig = (cfg: Awaited<ReturnType<typeof configGet>>) => {
    if (cfg.send_when_running) setSendPolicy(cfg.send_when_running);
    if (cfg.experimental) {
      setExperimental({
        automatic_knowledge_distillation:
          cfg.experimental.automatic_knowledge_distillation === true,
        browser_automation: cfg.experimental.browser_automation === true,
        remote_mcp: cfg.experimental.remote_mcp === true,
      });
    }
    setConfigLoaded(true);
    setConfigErr("");
  };

  const reloadConfig = async (): Promise<boolean> => {
    try {
      applyConfig(await configGet());
      return true;
    } catch (error) {
      setConfigLoaded(false);
      setConfigErr(error instanceof Error ? error.message : String(error));
      return false;
    }
  };

  const readinessStatusLabel = (status: RouteReadinessStatus) =>
    ({
      ready: "可派发",
      missing_binding: "缺少角色绑定",
      missing_credential: "缺少可用凭证",
      unknown_provider: "Provider 不存在",
      missing_custom_provider: "自定义 Provider 不存在",
      temporarily_unavailable: "因限流、并发、预算或 Circuit 暂不可用",
    })[status];

  // config 与当前 Workspace MRM readiness 合并重拉：onMount 首拉 + 断线 resync 对账。
  const reloadOverview = async () => {
    const [cfgResult, reportResult] = await Promise.allSettled([configGet(), mrmReadiness()]);
    const cfg = cfgResult.status === "fulfilled" ? cfgResult.value : null;
    const report = reportResult.status === "fulfilled" ? reportResult.value : null;
    if (cfg) applyConfig(cfg);
    else {
      setConfigLoaded(false);
      setConfigErr(
        cfgResult.status === "rejected"
          ? cfgResult.reason instanceof Error
            ? cfgResult.reason.message
            : String(cfgResult.reason)
          : "UNKNOWN",
      );
    }
    const detailFor = (roles: string[]) =>
      (report?.roles ?? [])
        .filter((role) => roles.includes(role.role) && !role.ready)
        .map((role) => `${role.role}: ${readinessStatusLabel(role.status)}`)
        .join("；") ?? "";
    setReadiness({
      workspace: report?.workspace_ready ?? null,
      chat: report?.chat_ready ?? null,
      agents: report?.agents_ready ?? null,
      chatDetail: report ? detailFor(["chat"]) : "",
      agentsDetail: report
        ? detailFor(["thinking", "planning", "execution", "review", "research"])
        : "",
    });
  };

  onMount(() => {
    void reloadOverview();
    // 保存 RPC 在飞时跳过 resync 对账，避免旧快照覆盖乐观显示值（同 KnowledgeBlockedPanel 的 busy 守卫）
    const offResync = client.onResync(() => {
      if (!policySaving() && experimentalSaving().size === 0) void reloadOverview();
    });
    onCleanup(offResync);
  });

  const setPolicy = async (p: string) => {
    if (!configLoaded() || policySaving() || p === sendPolicy()) return;
    const prev = sendPolicy();
    setSendPolicy(p);
    setPolicySaving(true);
    try {
      await client.rpc("config.set_send_policy", { policy: p });
    } catch (e) {
      if (!(await reloadConfig())) setSendPolicy(prev);
      flashErr(`保存失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setPolicySaving(false);
    }
  };

  type ExperimentalKey = keyof ReturnType<typeof experimental>;
  const setExperimentalFlag = async (key: ExperimentalKey, enabled: boolean) => {
    if (!configLoaded() || experimentalSaving().has(key)) return;
    const prev = experimental()[key];
    setExperimental((current) => ({ ...current, [key]: enabled }));
    setExperimentalSaving((current) => new Set(current).add(key));
    try {
      await client.rpc("config.set_experimental", { key, enabled });
    } catch (e) {
      // remote_mcp 可能已持久化、仅 runtime reload 失败；必须读回权威配置，不能盲目回滚。
      if (!(await reloadConfig())) setExperimental((current) => ({ ...current, [key]: prev }));
      flashErr(`保存失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setExperimentalSaving((current) => {
        const next = new Set(current);
        next.delete(key);
        return next;
      });
    }
  };

  const exportDiag = async () => {
    const r = await client.rpc<{ path: string }>("diagnostics.export").catch((e: unknown) => {
      flashErr(`导出诊断包失败：${formatError(e)}`);
      return null;
    });
    // web 模式拿到的是服务器本地路径，浏览器无法直接打开：提示语义明确，不静默
    if (r) flashOk(`已导出 ${r.path}${isWeb() ? "（服务器本地路径，浏览器端无法直接打开）" : ""}`);
  };

  return (
    <div class="h-full flex-1 overflow-auto">
      {/* 拖拽占位条只在 Tauri 无边框窗口下需要 */}
      <Show when={isTauri()}>
        <div class="h-8" data-tauri-drag-region onMouseDown={onDragStart} />
      </Show>
      <div class="px-8 py-6 pt-2 flex gap-8">
        <nav class="w-36 shrink-0 space-y-0.5" aria-label="设置导航">
          <h1 class="px-2.5 pb-2 text-lg font-medium text-[var(--text)]">设置</h1>
          <div role="tablist" aria-label="设置分类" aria-orientation="vertical">
            <For each={SECTIONS}>
              {(s, index) => (
                <button
                  id={`settings-tab-${index()}`}
                  role="tab"
                  aria-selected={section() === s}
                  aria-controls={`settings-panel-${index()}`}
                  tabIndex={section() === s ? 0 : -1}
                  class="w-full text-left px-2.5 py-1.5 rounded-md text-sm"
                  classList={{
                    "bg-[var(--bg-overlay)] text-[var(--text)]": section() === s,
                    "text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60": section() !== s,
                  }}
                  onClick={() => setSection(s)}
                  onKeyDown={onTabKeyDown}
                >
                  {s}
                </button>
              )}
            </For>
          </div>
        </nav>

        <div
          id={`settings-panel-${SECTIONS.indexOf(section())}`}
          role="tabpanel"
          aria-labelledby={`settings-tab-${SECTIONS.indexOf(section())}`}
          class="flex-1 min-w-0 space-y-4"
        >
          <Show when={configErr()}>
            <div class="rounded-md border border-[var(--err)]/50 px-3 py-2 text-xs text-[var(--err)]">
              配置读取失败，当前值为 UNKNOWN：{configErr()}
              <button class="ml-2 hover:underline" onClick={() => void reloadConfig()}>
                重试
              </button>
            </div>
          </Show>
          <Show when={section() === "通用"}>
            <GeneralSection
              readiness={readiness()}
              readinessDetails={{
                chat: readiness().chatDetail,
                agents: readiness().agentsDetail,
              }}
              sendPolicy={sendPolicy()}
              configLoaded={configLoaded()}
              policySaving={policySaving()}
              onPolicy={(policy) => void setPolicy(policy)}
              onRouting={() => setSection("模型路由")}
            />
          </Show>

          <Show when={section() === "提供商"}>
            <ProvidersSection />
          </Show>

          <Show when={section() === "模型路由"}>
            <RoutingSection />
          </Show>

          <Show when={section() === "语音"}>
            <VoiceSection />
          </Show>

          <Show when={section() === "用量与统计"}>
            <UsageSection />
          </Show>

          <Show when={section() === "知识库 OKF"}>
            <KnowledgeSection />
          </Show>

          <Show when={section() === "定时任务"}>
            <ScheduleSection />
          </Show>

          <Show when={section() === "诊断"}>
            <DoctorSection />
          </Show>

          <Show when={section() === "高级"}>
            <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 space-y-3 text-sm text-[var(--text-dim)]">
              <ComposerSuggestionsSection />
              <div class="border-t border-[var(--border)]" />
              <div class="text-sm text-[var(--text)]">审批规则与审计</div>
              <ApprovalRulesSection />
              <ApprovalHistorySection />
              <div class="border-t border-[var(--border)]" />
              <div class="space-y-2">
                <div class="text-sm text-[var(--text)]">实验能力与数据边界</div>
                <div class="text-xs text-[var(--text-faint)]">
                  模型调用会把你的消息、显式附件、注入知识和工具结果发送给当前会话显示的
                  Provider。以下能力会扩大自动外发或网络访问范围，默认全部关闭。
                </div>
                <For
                  each={
                    [
                      [
                        "automatic_knowledge_distillation",
                        "自动知识沉淀",
                        "每 30 分钟按各 Session 所属 Workspace 的模型路由，把近 24 小时活跃 Session 各自最近 20 条文本和注入上下文发送给对应 Provider，只写个人知识库",
                      ],
                      [
                        "browser_automation",
                        "Browser automation",
                        "允许 Agent 驱动本机 Chrome；全部 HTTP/S 和 WebSocket 请求经过受控代理，网页数据可能发送给 Provider",
                      ],
                      [
                        "remote_mcp",
                        "Remote MCP",
                        "允许 HTTP/SSE MCP server 接收工具参数；本地 stdio MCP 不受影响",
                      ],
                    ] as const
                  }
                >
                  {([key, label, hint]) => (
                    <div class="flex items-center justify-between gap-4 py-1.5">
                      <div>
                        <div class="text-xs text-[var(--text)]">{label}</div>
                        <div class="text-2xs text-[var(--text-faint)]">{hint}</div>
                      </div>
                      <button
                        class="pressable px-2.5 py-1 rounded-md text-xs border"
                        disabled={!configLoaded() || experimentalSaving().has(key)}
                        classList={{
                          "border-[var(--warn)] text-[var(--warn)]": experimental()[key],
                          "border-[var(--border)] text-[var(--text-dim)]": !experimental()[key],
                        }}
                        onClick={() => void setExperimentalFlag(key, !experimental()[key])}
                      >
                        {experimental()[key] ? "已启用" : "已关闭"}
                      </button>
                    </div>
                  )}
                </For>
              </div>
              <div class="pt-2 border-t border-[var(--border)]">
                <div>
                  hooks：`~/.agents/kxen/config.toml` 的 [hooks]（默认全关，pre_tool_use 可阻断）
                </div>
                <div>statusline：同文件 [statusline] items 白名单控制显隐</div>
              </div>
              <div class="pt-2 border-t border-[var(--border)]">
                <McpSection />
              </div>
              <div class="pt-1 border-t border-[var(--border)] flex items-center gap-3">
                <button
                  class="pressable px-3 py-1.5 rounded-md text-xs border border-[var(--border)] text-[var(--text)]"
                  onClick={() => void exportDiag()}
                >
                  导出诊断包（markdown）
                </button>
              </div>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}
