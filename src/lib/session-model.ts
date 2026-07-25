// 会话级模型（P1-02）：切换写 session metadata；全局默认走设置页 config.set_role，保持原路径。
import { createEffect, createSignal } from "solid-js";
import { client } from "./client";
import { currentModel } from "./chat";
import { displayName, modelsCatalog } from "./models";

// 草稿态（会话未落库）的模型选择无处可写：暂存于此，会话创建后写入其 metadata；
// "default" = 暂存的是「跟随全局默认」（清除覆盖），与具体模型二选一
let draftPick: { provider: string; model: string } | "default" | null = null;

export async function sessionSetModel(
  sessionId: string,
  provider: string,
  model: string,
): Promise<void> {
  if (!sessionId) {
    draftPick = { provider, model };
    return;
  }
  return client.rpc("session.set_model", { id: sessionId, provider, model });
}

/** 清除会话级覆盖，跟随全局默认（后端约定：provider/model 同缺 = 清除）。 */
export async function sessionFollowGlobalModel(sessionId: string): Promise<void> {
  if (!sessionId) {
    draftPick = "default";
    return;
  }
  return client.rpc("session.set_model", { id: sessionId });
}

/** 会话落库后回写草稿态选择的模型（ensureActiveSession 创建会话后调用）。 */
export async function applyDraftModel(sessionId: string): Promise<void> {
  const pick = draftPick;
  draftPick = null;
  if (pick === "default") await sessionFollowGlobalModel(sessionId).catch(() => {});
  else if (pick) await sessionSetModel(sessionId, pick.provider, pick.model).catch(() => {});
}

/** 当前 session 生效模型的显示名（session 覆盖 > 全局默认；切会话自动重取）。 */
export function createSessionModelLabel(getSid: () => string): () => string {
  const [label, setLabel] = createSignal("");
  createEffect(() => {
    void currentModel(getSid() || undefined).then(async (m) =>
      setLabel(displayName(await modelsCatalog().catch(() => []), m.provider, m.model)),
    );
  });
  return label;
}
