// Tauri invoke 封装（浏览器 dev 环境降级为 mock）。
import { invoke as tauriInvoke } from '@tauri-apps/api/core';

export interface DoctorEntry {
  provider: string;
  display: string;
  status: 'imported' | 'ok' | 'missing' | 'expired';
  detail: string;
}

export interface DoctorReport {
  bun_like_runtime: string;
  data_dir: string;
  config_dir: string;
  entries: DoctorEntry[];
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function doctor(): Promise<DoctorReport> {
  if (isTauri) {
    return tauriInvoke<DoctorReport>('doctor');
  }
  // 浏览器 dev 预览 mock
  return {
    bun_like_runtime: 'rust 1.96 (browser mock)',
    data_dir: '~/Library/Application Support/kxen',
    config_dir: '~/.config/kxen',
    entries: [
      { provider: 'anthropic', display: 'Claude Pro/Max', status: 'ok', detail: 'credential present' },
      { provider: 'openai', display: 'ChatGPT Plus/Pro (codex)', status: 'missing', detail: 'no credential found' },
      { provider: 'xai', display: 'SuperGrok (grok-build)', status: 'ok', detail: 'credential present' },
      { provider: 'kimi-for-coding', display: 'Kimi Code', status: 'ok', detail: 'credential present' },
    ],
  };
}
