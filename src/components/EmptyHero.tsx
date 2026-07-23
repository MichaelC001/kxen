// 空态：logo + 四快捷卡。
// 入场动画只在 app 首次挂载播放；之后点新会话直接静态到位
//（旧时间线清空与空态呈现同帧完成，不再经历 300ms 空白 + 闪入的割裂感）。
import { onMount } from "solid-js";
import { Target, Users, Workflow, Wrench } from "lucide-solid";

const CARDS = [
  { icon: Target, title: "write-goal", desc: "定义带完成判据的目标，自动推进直到验证通过" },
  { icon: Wrench, title: "@ 与 /", desc: "@ 引用文件目录，/ 唤起命令与 skills" },
  { icon: Workflow, title: "workflow", desc: "我自己写编排脚本，并行派发多个子代理" },
  { icon: Users, title: "agent teams", desc: "spawn 多模型 teammates 组队干活，各自独立上下文" },
];

let heroPlayed = false;

export default function EmptyHero() {
  const animated = !heroPlayed;
  onMount(() => {
    heroPlayed = true;
  });
  return (
    <div class="pt-16 space-y-8 w-full">
      <div class={animated ? "empty-hero" : ""} classList={{ "flex items-center gap-4": true }}>
        <img
          src="/icon.png"
          alt="kxen"
          class="w-14 h-14 rounded-2xl shadow-lg shadow-indigo-500/20"
        />
        <div>
          <div class="text-lg font-semibold tracking-tight">kxen</div>
          <div class="text-xs text-[var(--text-dim)]">多模型并行工作 · 目标驱动 · 团队编排</div>
        </div>
      </div>
      <div class="grid grid-cols-2 gap-2.5">
        {CARDS.map((c, i) => (
          <div
            class={`rounded-xl border border-[var(--border)] bg-[var(--bg-raised)] p-3.5 space-y-1.5 ${animated ? "empty-card" : ""}`}
            style={animated ? `animation-delay: ${80 + i * 50}ms` : ""}
          >
            <c.icon size={16} class="text-[var(--accent-hover)]" />
            <div class="text-xs font-medium font-mono">{c.title}</div>
            <div class="text-xs leading-snug text-[var(--text-faint)]">{c.desc}</div>
          </div>
        ))}
      </div>
      <div
        class={`text-xs text-[var(--text-faint)] ${animated ? "empty-card" : ""}`}
        style={animated ? "animation-delay: 300ms" : ""}
      >
        输入消息开始 · @ 引用 · / 命令 · # 沉淀 · 粘贴图片
      </div>
    </div>
  );
}
