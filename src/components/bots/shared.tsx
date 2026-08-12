import type { JSX } from "solid-js";
import type { BotActor, BotMessagePart } from "../../lib/bots";

export interface RefreshProps {
  epoch: number;
  onChanged: () => void;
}

export const actionClass =
  "pressable px-2.5 py-1 rounded border border-[var(--border)] text-xs text-[var(--text-dim)] hover:text-[var(--text)] disabled:opacity-40";
export const primaryClass =
  "pressable px-2.5 py-1 rounded bg-[var(--accent)] text-[var(--accent-contrast)] text-xs disabled:opacity-40";
export const fieldClass =
  "w-full rounded border border-[var(--border)] bg-transparent px-2.5 py-1.5 text-xs text-[var(--text)] outline-none focus:border-[var(--accent)]";

export function Panel(props: { title: string; detail?: string; children: JSX.Element }) {
  return (
    <section class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4">
      <h2 class="text-sm font-medium">{props.title}</h2>
      {props.detail && <p class="text-xs text-[var(--text-faint)] mt-0.5 mb-3">{props.detail}</p>}
      <div class={props.detail ? "" : "mt-3"}>{props.children}</div>
    </section>
  );
}

export function actorLabel(actor: BotActor): string {
  if (actor.kind === "owner") return "Owner";
  if (actor.kind === "bot") return actor.id;
  if (actor.kind === "system") return `System:${actor.actor}`;
  return "Agent";
}

export function partText(parts: BotMessagePart[]): string {
  return parts
    .map((part) => {
      if (part.kind === "text") return part.text;
      if (part.kind === "artifact_ref") return `[Artifact] ${part.artifact.display_name}`;
      return `[${part.schema_id}] ${Object.entries(part.fields)
        .map(([key, value]) => `${key}=${value}`)
        .join(", ")}`;
    })
    .join("\n");
}

export function statusClass(status: string): string {
  if (["active", "completed", "PASS"].includes(status)) return "text-[var(--ok)]";
  if (["failed", "rejected", "blocked", "FAIL"].includes(status)) return "text-[var(--err)]";
  if (["approval_required", "input_required", "UNKNOWN"].includes(status))
    return "text-[var(--warn)]";
  return "text-[var(--text-faint)]";
}

export function shortId(value: string): string {
  return value.length > 24 ? `${value.slice(0, 12)}...${value.slice(-8)}` : value;
}
