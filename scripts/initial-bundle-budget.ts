import { gzipSync } from "node:zlib";

interface BundleChunk {
  type: "chunk";
  fileName: string;
  code: string;
  imports: string[];
  isEntry: boolean;
  moduleIds: string[];
}

interface BundleAsset {
  type: "asset";
}

type BundleOutput = BundleChunk | BundleAsset;

export interface InitialBundleBudgetOptions {
  label: string;
  rawBytes: number;
  gzipBytes: number;
  forbiddenModules?: RegExp[];
}

/**
 * Limit every browser entry's transitive static import graph. Dynamic imports are
 * intentionally excluded so heavy renderers remain available without taxing startup.
 */
export function initialBundleBudget(options: InitialBundleBudgetOptions) {
  return {
    name: `kxen-initial-bundle-budget-${options.label}`,
    generateBundle(_outputOptions: unknown, bundle: Record<string, BundleOutput>) {
      const chunks = new Map(
        Object.values(bundle)
          .filter((output): output is BundleChunk => output.type === "chunk")
          .map((chunk) => [chunk.fileName, chunk]),
      );
      for (const entry of chunks.values().filter((chunk) => chunk.isEntry)) {
        const initial = staticClosure(entry, chunks);
        const rawBytes = initial.reduce((total, chunk) => total + Buffer.byteLength(chunk.code), 0);
        const gzipBytes = initial.reduce(
          (total, chunk) => total + gzipSync(Buffer.from(chunk.code)).length,
          0,
        );
        const forbidden = initial.flatMap((chunk) =>
          chunk.moduleIds.filter((moduleId) =>
            options.forbiddenModules?.some((pattern) => pattern.test(moduleId)),
          ),
        );
        if (forbidden.length > 0) {
          throw new Error(
            `${options.label} initial entry ${entry.fileName} statically loads deferred modules: ${[...new Set(forbidden)].join(", ")}`,
          );
        }
        if (rawBytes > options.rawBytes || gzipBytes > options.gzipBytes) {
          throw new Error(
            `${options.label} initial entry ${entry.fileName} exceeds budget: raw=${rawBytes}/${options.rawBytes} gzip=${gzipBytes}/${options.gzipBytes}; chunks=${initial.map((chunk) => chunk.fileName).join(",")}`,
          );
        }
      }
    },
  };
}

function staticClosure(entry: BundleChunk, chunks: Map<string, BundleChunk>): BundleChunk[] {
  const visited = new Map<string, BundleChunk>();
  const visit = (chunk: BundleChunk) => {
    if (visited.has(chunk.fileName)) return;
    visited.set(chunk.fileName, chunk);
    for (const imported of chunk.imports) {
      const dependency = chunks.get(imported);
      if (dependency) visit(dependency);
    }
  };
  visit(entry);
  return [...visited.values()];
}
