// 普通文件附件的路径解析：浏览器 File 只暴露 basename + size，
// 真实位置靠后端 workspace 索引反查（fs.resolve_name），同名文件按 size 消歧。
import { client } from "../../lib/client";

export interface NameMatch {
  path: string;
  size: number;
}

export async function fsResolveName(name: string): Promise<NameMatch[]> {
  return client.rpc<NameMatch[]>("fs.resolve_name", { name });
}

/** 相对路径守卫：拒绝对路径、盘符、反斜杠与 .. 段（防逃逸；后端 agent/context.rs 还会再拦一次）。 */
export function isSafeRelPath(p: string): boolean {
  if (!p || p.includes("\\")) return false;
  if (p.startsWith("/") || /^[A-Za-z]:/.test(p)) return false;
  return !p.split("/").some((seg) => seg === "..");
}

/** File -> workspace 相对路径：basename 精确匹配，多命中按 size 消歧；无法唯一确定返回 null。 */
export function resolveAttachPath(
  name: string,
  size: number,
  candidates: NameMatch[],
): string | null {
  const named = candidates.filter((c) => isSafeRelPath(c.path) && c.path.split("/").pop() === name);
  // 唯一同名直接采用：size 不一致只是文件在选取后被改写，读取以发送时内容为准
  const onlyNamed = named[0];
  if (named.length === 1 && onlyNamed) return onlyNamed.path;
  const sized = named.filter((c) => c.size === size);
  const onlySized = sized[0];
  return sized.length === 1 && onlySized ? onlySized.path : null;
}
