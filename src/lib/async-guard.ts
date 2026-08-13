// 统一异步守卫：seq 过期丢弃与 in-flight 去重。
import { createSignal } from "solid-js";
import { formatError } from "./error-text";
import { flashErr, flashOk } from "./flash";

/** seq 过期守卫：只有最后一次发起的结果允许落地（RPC 慢响应覆盖新弹层/新查询的通病的解药）。 */
export function createSeqGuard(): { next: () => number; isCurrent: (id: number) => boolean } {
  let seq = 0;
  return { next: () => ++seq, isCurrent: (id) => id === seq };
}

/** in-flight 去重：同 key 的并发调用共享同一个 Promise（防止连点/双触发产生并发写）。 */
export function createInFlight() {
  const pending = new Map<string, Promise<unknown>>();
  return function dedupe<T>(key: string, fn: () => Promise<T>): Promise<T> {
    const hit = pending.get(key);
    if (hit) return hit as Promise<T>;
    const p = fn().finally(() => pending.delete(key));
    pending.set(key, p);
    return p;
  };
}

/** 异步操作三态：pending 禁用 / 失败回滚 / 错误 flash，一行接入替代手写 try-catch-finally。 */
export function createAction(): {
  pending: () => boolean;
  run: <T>(
    task: () => Promise<T>,
    opts?: { okText?: string; errPrefix?: string; onOk?: (r: T) => void; onErr?: () => void },
  ) => Promise<T | undefined>;
} {
  const [pending, setPending] = createSignal(false);
  const run = async <T>(
    task: () => Promise<T>,
    opts: { okText?: string; errPrefix?: string; onOk?: (r: T) => void; onErr?: () => void } = {},
  ): Promise<T | undefined> => {
    if (pending()) return undefined; // 三态之「进行中禁用」：连点直接拒
    setPending(true);
    try {
      const r = await task();
      if (opts.okText) flashOk(opts.okText);
      opts.onOk?.(r);
      return r;
    } catch (e) {
      flashErr(`${opts.errPrefix ?? "操作失败"}：${e instanceof Error ? e.message : String(e)}`);
      opts.onErr?.();
      return undefined;
    } finally {
      setPending(false);
    }
  };
  return { pending, run };
}

export interface ReconciledMutation<T> {
  key: string;
  prepare: () => T;
  execute: (token: T) => Promise<unknown>;
  applied?: (token: T) => boolean;
  onApplied?: (token: T) => void;
  okText: string;
  errPrefix?: string;
}

/**
 * Durable mutation controller. A semantic operation key keeps generated resource and
 * idempotency IDs stable across retries, then an authoritative refresh decides whether
 * an ambiguous error was already committed.
 */
export function createReconciledMutation(options: {
  refresh: () => Promise<unknown>;
  onChanged: () => void;
}) {
  const [pending, setPending] = createSignal(false);
  const retryTokens = new Map<string, unknown>();

  const run = async <T>(
    mutation: ReconciledMutation<T>,
  ): Promise<"applied" | "failed" | "busy"> => {
    if (pending()) return "busy";
    setPending(true);
    let token: T;
    try {
      token = retryTokens.has(mutation.key)
        ? (retryTokens.get(mutation.key) as T)
        : mutation.prepare();
    } catch (error) {
      setPending(false);
      flashErr(`${mutation.errPrefix ?? `${mutation.okText}失败`}：${formatError(error)}`);
      return "failed";
    }
    retryTokens.set(mutation.key, token);
    let mutationFailed = false;
    let mutationError: unknown;
    let refreshError: unknown;
    try {
      await mutation.execute(token);
    } catch (error) {
      mutationFailed = true;
      mutationError = error;
    }
    try {
      await options.refresh();
    } catch (error) {
      refreshError = error;
    }
    let reconciled = false;
    if (mutationFailed && mutation.applied) {
      try {
        reconciled = mutation.applied(token);
      } catch (error) {
        refreshError ??= error;
      }
    }
    const applied = !mutationFailed || reconciled;
    try {
      if (applied) {
        retryTokens.delete(mutation.key);
        mutation.onApplied?.(token);
        options.onChanged();
        flashOk(reconciled ? `${mutation.okText}，已从 durable state 确认` : mutation.okText);
        return "applied";
      }
      const refreshDetail = refreshError ? `；状态重新读取失败：${formatError(refreshError)}` : "";
      flashErr(
        `${mutation.errPrefix ?? `${mutation.okText}失败`}：${formatError(mutationError)}${refreshDetail}`,
      );
      return "failed";
    } finally {
      setPending(false);
    }
  };

  const reset = () => retryTokens.clear();
  return { pending, run, reset, hasRetry: (key: string) => retryTokens.has(key) };
}

export type ReconciledMutationController = ReturnType<typeof createReconciledMutation>;
