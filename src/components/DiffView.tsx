// @pierre/diffs 的 Solid 挂载壳：统一收敛应用内所有 diff 渲染（工具卡内联 diff、dock 改动面板）。
// 两种输入：old/new 文件内容对（agent 场景无线性历史，直接快照对比），或统一 patch 文本。
// 主题跟随应用 data-theme（github-dark/light，与 Markdown 代码块的 shiki 主题一致）。
// 库本体（含 shiki）体积大：动态 import 按需加载，不进主 chunk（构建有 500KB 预算门禁）。
import { createEffect, onCleanup, onMount } from "solid-js";
import type { FileContents, FileDiff } from "@pierre/diffs";

export default function DiffView(props: {
  oldFile?: FileContents;
  newFile?: FileContents;
  /** 统一 diff/patch 文本（单文件）；与 oldFile/newFile 二选一 */
  patch?: string;
}) {
  let ref: HTMLDivElement | undefined;
  let instance: FileDiff | undefined;
  let parsePatchFiles: typeof import("@pierre/diffs").parsePatchFiles | undefined;
  let observer: MutationObserver | undefined;
  let disposed = false;

  const themeType = () =>
    (document.documentElement.dataset.theme === "light" ? "light" : "dark") as "light" | "dark";

  const renderInput = (
    container: HTMLElement,
    patch: string | undefined,
    oldFile: FileContents | undefined,
    newFile: FileContents | undefined,
  ) => {
    if (!instance || !parsePatchFiles) return;
    if (patch !== undefined) {
      const parsed = parsePatchFiles(patch)[0]?.files[0];
      if (parsed) instance.render({ fileDiff: parsed, containerWrapper: container });
    } else if (oldFile && newFile) {
      instance.render({ oldFile, newFile, containerWrapper: container });
    }
  };

  onMount(async () => {
    if (!ref) return;
    const container = ref;
    const mod = await import("@pierre/diffs");
    // await 期间组件已卸载：不得再触碰 DOM
    if (disposed) return;
    parsePatchFiles = mod.parsePatchFiles;
    instance = new mod.FileDiff({
      theme: { dark: "github-dark", light: "github-light" },
      themeType: themeType(),
      diffStyle: "unified",
      disableFileHeader: true,
      overflow: "scroll",
    });
    renderInput(container, props.patch, props.oldFile, props.newFile);
    observer = new MutationObserver(() => instance?.setThemeType(themeType()));
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
  });
  // 宿主普遍用非 keyed Show 包 DiffView：props 换文件不重建组件，必须跟踪输入重渲染
  createEffect(() => {
    const patch = props.patch;
    const oldFile = props.oldFile;
    const newFile = props.newFile;
    if (ref) renderInput(ref, patch, oldFile, newFile);
  });
  onCleanup(() => {
    disposed = true;
    observer?.disconnect();
    instance?.cleanUp();
  });

  return <div ref={(el) => (ref = el)} class="diff-view min-w-0" />;
}
