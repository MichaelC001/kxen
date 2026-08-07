/** dock 分区卡片骨架：图标标题 + 内容（Dock 各分区与 DockGoal 共用）。 */
export default function DockSection(props: {
  title: string;
  icon: (p: { size: number; class?: string }) => import("solid-js").JSX.Element;
  children: import("solid-js").JSX.Element;
}) {
  const Icon = props.icon;
  return (
    <div class="border-b border-[var(--border)] px-3 py-3">
      <div class="text-2xs uppercase tracking-wider text-[var(--text-faint)] mb-2 flex items-center gap-1.5">
        <Icon size={11} class="text-[var(--text-faint)]" />
        {props.title}
      </div>
      {props.children}
    </div>
  );
}
