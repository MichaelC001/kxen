import { createSignal, For, Show } from "solid-js";
import { ChevronRight } from "lucide-solid";
import { statusDot } from "../lib/variants";
import {
  expandAllTools,
  parseToolDiff,
  parseWorkflowSubcalls,
  toolArgsPath,
  toolMetaBadge,
  workflowScript,
} from "../lib/tool-ui";
import { openToolPath } from "../lib/file-open";
import DiffView from "./DiffView";

/** 工具活动卡：头部行（状态点 + 名称 + 参数摘要 + 元信息徽标 + 展开箭头）整行即开关；
 *  展开体分 IN（精确 arguments + 可点击文件路径）/ OUT（完整结果）两区——调用和结果是一个整体。
 *  失败（ERROR 前缀 / interrupted）在折叠行以红色状态点外露；edit/write 展开后渲染结构化 diff（@pierre/diffs）。
 *  折叠是受控的：本地手动开合优先，未手动操作过则跟随全局「展开全部」（Ctrl+O）；
 *  不用原生 toggle 事件驱动，避免全局翻转时被动事件覆盖用户意图。 */
export default function ToolCard(props: {
  name: string;
  call: string;
  args?: string | undefined;
  result?: string | undefined;
  /** Chat -> Trajectory 检视联动入口（hover 露出）；无落盘定位信息的流式条目不传 */
  onInspect?: (() => void) | undefined;
}) {
  const [localOpen, setLocalOpen] = createSignal<boolean | undefined>(undefined);
  const open = () => localOpen() ?? expandAllTools();
  const failed = () => props.result?.startsWith("ERROR") || props.result === "interrupted";
  const badge = () =>
    toolMetaBadge({
      kind: "tool",
      name: props.name,
      call: props.call,
      args: props.args,
      result: props.result,
    });
  // 展开时才解析 diff：折叠态零成本；解析失败回落原文 pre
  const diff = () => (open() ? parseToolDiff(props.name, props.args, props.result) : undefined);
  // workflow 工具行：脚本源码（args.script）+ 脚本内子调用列表（结果尾部结构化块），展开时才解析
  const isWorkflow = () => props.name === "workflow";
  const script = () => (isWorkflow() && open() ? workflowScript(props.args) : undefined);
  const subcalls = () => (isWorkflow() && open() ? parseWorkflowSubcalls(props.result) : []);
  // 精确 arguments 里的文件路径：展开体 IN 区可点击跳转（桌面打开 / web 复制兜底）
  const path = () => toolArgsPath(props.args);
  return (
    <details
      class="group rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] text-xs overflow-hidden"
      open={open()}
    >
      <summary
        class="flex items-center gap-2 px-3 py-1.5 cursor-pointer select-none list-none"
        onClick={(e) => {
          e.preventDefault();
          setLocalOpen(!open());
        }}
      >
        <span
          class={statusDot({
            tone: props.result === undefined ? "warn" : failed() ? "err" : "ok",
            pulse: props.result === undefined,
          })}
        />
        <span class="font-mono text-[var(--accent-hover)]">{props.name}</span>
        {/* 动态工具（dyn__*）：模型运行时定义、审批后生效，名字内含实现 hash */}
        <Show when={props.name.startsWith("dyn__")}>
          <span class="text-2xs shrink-0 rounded border border-[var(--accent)]/40 px-1 text-[var(--accent-hover)]">
            动态
          </span>
        </Show>
        <span class="text-[var(--text-dim)] truncate flex-1 font-mono">{props.call}</span>
        <Show when={badge()}>
          <span class="text-2xs tabular-nums text-[var(--text-faint)] shrink-0">{badge()}</span>
        </Show>
        <Show when={props.onInspect}>
          <button
            type="button"
            data-testid="tool-inspect"
            class="pressable shrink-0 px-1.5 rounded border border-[var(--border)] text-2xs text-[var(--text-dim)] opacity-0 group-hover:opacity-100 transition-opacity"
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              props.onInspect?.();
            }}
          >
            Inspect
          </button>
        </Show>
        <ChevronRight
          size={12}
          class="text-[var(--text-faint)] transition-transform duration-150 shrink-0"
          classList={{ "rotate-90": open() }}
        />
      </summary>
      <Show when={open()}>
        <Show
          when={diff()}
          fallback={
            <>
              {/* workflow：子调用列表（缩进嵌套：状态点 + 名称 + cached/耗时徽标） */}
              <Show when={subcalls().length > 0}>
                <div class="border-t border-[var(--border)] px-3 py-1.5">
                  <div class="text-2xs text-[var(--text-faint)] pb-1">脚本内工具调用</div>
                  <For each={subcalls()}>
                    {(s) => (
                      <div
                        class="flex items-center gap-2 pl-3 py-0.5"
                        data-testid="workflow-subcall"
                      >
                        <span
                          class={statusDot({
                            tone: s.status === "ok" ? "ok" : "err",
                            pulse: false,
                          })}
                        />
                        <span class="font-mono text-[var(--accent-hover)]">{s.name}</span>
                        <Show when={s.cached}>
                          <span class="text-2xs text-[var(--text-faint)]">cached</span>
                        </Show>
                        {/* 无耗时不虚构：缓存回放/缺失一律留空 */}
                        <span class="text-2xs tabular-nums text-[var(--text-faint)]">
                          {s.ms !== undefined ? `${s.ms}ms` : ""}
                        </span>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
              {/* workflow：脚本源码独立成区，默认再折一层（卡展开后仍不铺开长脚本） */}
              <Show when={script()}>
                {(s) => (
                  <details class="border-t border-[var(--border)]">
                    <summary class="px-3 py-1.5 cursor-pointer select-none text-2xs text-[var(--text-faint)]">
                      脚本源码
                    </summary>
                    <pre class="selectable px-3 py-2 bg-[var(--code-bg)] text-[var(--text-dim)] whitespace-pre-wrap break-all max-h-64 overflow-auto">
                      {s()}
                    </pre>
                  </details>
                )}
              </Show>
              {/* 持久化的精确 arguments（流式态没有，对账后由存储快照补上）；workflow 的 args 已由脚本源码区承载。
                  IN 区标题旁的文件路径可点击跳转（desktop 打开 / web 复制） */}
              <Show when={props.args && !isWorkflow()}>
                <div class="border-t border-[var(--border)]">
                  <div class="flex items-center gap-2 px-3 pt-1.5 text-2xs text-[var(--text-faint)]">
                    <span class="shrink-0">IN</span>
                    <Show when={path()}>
                      {(p) => (
                        <button
                          type="button"
                          data-testid="tool-path"
                          title={p()}
                          class="pressable font-mono text-[var(--accent-hover)] hover:underline truncate"
                          onClick={() => void openToolPath(p())}
                        >
                          {p()}
                        </button>
                      )}
                    </Show>
                  </div>
                  <pre class="selectable px-3 py-2 bg-[var(--code-bg)] text-[var(--text-dim)] whitespace-pre-wrap break-all max-h-64 overflow-auto">
                    {props.args}
                  </pre>
                </div>
              </Show>
              <Show when={props.result !== undefined}>
                <div class="border-t border-[var(--border)]">
                  <div class="px-3 pt-1.5 text-2xs text-[var(--text-faint)]">OUT</div>
                  <pre class="selectable px-3 py-2 bg-[var(--code-bg)] text-[var(--text-dim)] whitespace-pre-wrap break-all max-h-64 overflow-auto">
                    {props.result}
                  </pre>
                </div>
              </Show>
            </>
          }
        >
          {(d) => (
            <div class="border-t border-[var(--border)] max-h-72 overflow-auto">
              <DiffView
                oldFile={{ name: d().path ?? "file", contents: d().oldText }}
                newFile={{ name: d().path ?? "file", contents: d().newText }}
              />
            </div>
          )}
        </Show>
      </Show>
    </details>
  );
}
