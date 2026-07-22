// 内置模型清单（ModelPicker 与 Cmd-K 面板共享）。
export interface ModelPreset {
  provider: string;
  brand: string;
  model: string;
  label: string;
  context: string;
  note: string;
}

export const PRESETS: ModelPreset[] = [
  {
    provider: "anthropic",
    brand: "Claude",
    model: "claude-sonnet-4-5-20250929",
    label: "Sonnet 4.5",
    context: "200k",
    note: "订阅 · 均衡主力",
  },
  {
    provider: "anthropic",
    brand: "Claude",
    model: "claude-opus-4-5-20251101",
    label: "Opus 4.5",
    context: "200k",
    note: "订阅 · 深度思考",
  },
  {
    provider: "openai",
    brand: "GPT",
    model: "gpt-5.4",
    label: "GPT-5.4",
    context: "400k",
    note: "Codex 订阅",
  },
  {
    provider: "xai",
    brand: "Grok",
    model: "grok-build-0.1",
    label: "Grok Build",
    context: "—",
    note: "订阅 · 高速执行",
  },
  {
    provider: "kimi-for-coding",
    brand: "Kimi",
    model: "kimi-for-coding",
    label: "Kimi Code",
    context: "256k",
    note: "订阅 · 工具调用强",
  },
];
