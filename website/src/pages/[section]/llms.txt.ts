import { getIndexedTopLevel, type IndexedEntry } from "@cloudflare/nimbus-docs";
import { utf8Text } from "@/lib/text-response";
import { config } from "virtual:nimbus/config";

export const prerender = true;

interface SectionProps {
  slug: string;
  label: string;
  members: IndexedEntry[];
}

export async function getStaticPaths() {
  const { groups } = await getIndexedTopLevel();
  return (
    groups
      // hidden 版本不进 agent 发现面（仍可直接 URL 访问）。
      .filter((group) => !group.hidden)
      .map((group) => ({
        params: { section: group.slug },
        props: {
          slug: group.slug,
          label: group.label,
          members: group.members,
        } as SectionProps,
      }))
  );
}

export async function GET({ props }: { props: SectionProps }) {
  const { label, members } = props;

  const lines = [`# ${label}`, "", "## Pages", ""];

  for (const item of members) {
    const description = item.description ? ` — ${item.description}` : "";
    lines.push(`- [${item.title}](${new URL(item.markdownUrl, config.site).href})${description}`);
  }

  lines.push("");

  return utf8Text(lines.join("\n"), "text/plain");
}
