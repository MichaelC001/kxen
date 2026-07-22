// 空态：logo + 四快捷卡（stagger 入场）。
import { Target, Users, Workflow, Wrench } from "lucide-solid";

const CARDS = [
  { icon: Target, title: "write-goal", desc: "定义带完成判据的目标，自动推进直到验证通过" },
  { icon: Wrench, title: "@ 与 /", desc: "@ 引用文件目录，/ 唤起命令与 skills" },
  { icon: Workflow, title: "workflow", desc: "我自己写编排脚本，并行派发多个子代理" },
  { icon: Users, title: "agent teams", desc: "spawn 多模型 teammates 组队干活，各自独立上下文" },
];

export default function EmptyHero() {
  return (
    <div class="pt-16 space-y-8 w-full">
      <div class="empty-hero flex items-center gap-4">
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
            class="empty-card rounded-xl border border-[var(--border)] bg-[var(--bg-raised)] p-3.5 space-y-1.5"
            style={`animation-delay: ${80 + i * 50}ms`}
          >
            <c.icon size={16} class="text-[var(--accent-hover)]" />
            <div class="text-xs font-medium font-mono">{c.title}</div>
            <div class="text-xs leading-snug text-[var(--text-faint)]">{c.desc}</div>
          </div>
        ))}
      </div>
      <div class="empty-card text-xs text-[var(--text-faint)]" style="animation-delay: 300ms">
        输入消息开始 · @ 引用 · / 命令 · # 沉淀 · 粘贴图片
      </div>
    </div>
  );
}
