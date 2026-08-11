// cva 变体定义：组件样式变体的唯一来源（类型化）。
import { cva } from "class-variance-authority";

export const sendBtn = cva("send-btn", {
  variants: {
    intent: {
      primary: "",
      danger: "send-btn-stop",
    },
  },
  defaultVariants: { intent: "primary" },
});

export const composerCard = cva("composer-card", {
  variants: {
    state: {
      default: "",
      recording: "recording",
    },
  },
  defaultVariants: { state: "default" },
});

/** 状态点：tool 卡 / 代理窗格 / 成员列表共用，避免三处 tone 色漂移。 */
export const statusDot = cva("inline-block w-1.5 h-1.5 rounded-full shrink-0", {
  variants: {
    tone: {
      ok: "bg-[var(--ok)]",
      warn: "bg-[var(--warn)]",
      err: "bg-[var(--err)]",
      accent: "bg-[var(--accent)]",
      faint: "bg-[var(--text-faint)]",
    },
    pulse: {
      true: "animate-pulse",
      false: "",
    },
  },
  defaultVariants: { tone: "faint", pulse: false },
});

export const badgeChip = cva("text-2xs px-1 rounded border border-[var(--border)]", {
  variants: {
    tone: {
      faint: "text-[var(--text-faint)]",
      accent: "text-[var(--accent-hover)] border-[var(--accent)]",
      warn: "text-[var(--warn)] border-[var(--warn)]/40",
    },
  },
  defaultVariants: { tone: "faint" },
});

export const popupItem = cva("w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs", {
  variants: {
    selected: {
      true: "bg-[var(--bg-overlay)]",
      false: "text-[var(--text-dim)]",
    },
  },
  defaultVariants: { selected: false },
});
