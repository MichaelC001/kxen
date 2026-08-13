export function BotRunMetric(props: { label: string; value: string; tone?: string }) {
  return (
    <div>
      <div class="text-2xs text-[var(--text-faint)]">{props.label}</div>
      <div class={props.tone || "text-[var(--text)]"}>{props.value}</div>
    </div>
  );
}

export function terminalRunStatus(status: string): boolean {
  return ["completed", "failed", "canceled", "rejected", "blocked"].includes(status);
}
