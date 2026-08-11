import { getIndexedTopLevel } from "@cloudflare/nimbus-docs";
import { utf8Text } from "@/lib/text-response";
import { config } from "virtual:nimbus/config";

export const prerender = true;

export async function GET() {
  const { leaves, groups } = await getIndexedTopLevel();

  const lines = [
    `# ${config.title}`,
    "",
    config.description ?? "Documentation index for AI agents.",
    "",
    `Full corpus (all pages, one document): ${new URL("/llms-full.txt", config.site).href}`,
    "",
    "## Pages",
    "",
  ];

  type Row = { key: string; line: string };
  const rows: Row[] = [
    {
      key: "/",
      line: `- [${config.title}](${new URL("/index.md", config.site).href}) — ${config.description}`,
    },
  ];

  for (const leaf of leaves) {
    const description = leaf.description ? ` — ${leaf.description}` : "";
    rows.push({
      key: leaf.url,
      line: `- [${leaf.title}](${new URL(leaf.markdownUrl, config.site).href})${description}`,
    });
  }

  for (const group of groups) {
    // 旧版本文档有独立 /<v>/llms.txt，根索引不重复列出。
    if (group.kind === "version") continue;
    rows.push({
      key: `/${group.slug}`,
      line: `- [${group.label}](${new URL(`/${group.slug}/llms.txt`, config.site).href})`,
    });
  }

  rows.sort((a, b) => a.key.localeCompare(b.key));
  for (const row of rows) lines.push(row.line);

  lines.push("");

  return utf8Text(lines.join("\n"), "text/plain");
}
