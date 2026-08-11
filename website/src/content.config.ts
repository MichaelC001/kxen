import { defineCollection } from "astro:content";
// astro:content 再导出的 z 已弃用，应从 astro/zod 导入（nimbus-docs schema 惯例）。
import { z } from "astro/zod";
import { docsCollection } from "@cloudflare/nimbus-docs/content";

export const collections = {
  docs: defineCollection(
    docsCollection({
      schemaFields: {
        audience: z.literal("human").optional(),
        status: z.enum(["canonical", "distilled", "archived", "superseded"]).optional(),
      },
    }),
  ),
};
