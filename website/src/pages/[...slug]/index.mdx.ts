import { getIndexedEntries, type IndexedEntry } from "@cloudflare/nimbus-docs";
import { utf8Text } from "@/lib/text-response";
import { config } from "virtual:nimbus/config";

export const prerender = true;

const PRIMARY_COLLECTION = "docs";

interface SlugProps {
  item: IndexedEntry;
}

export async function getStaticPaths() {
  const indexed = await getIndexedEntries();
  return indexed
    .filter((item) => item.collection === PRIMARY_COLLECTION && item.sourceUrl !== undefined)
    .map((item) => ({
      // index 页 slug 用 undefined，避免 rest 段变成 /index/index.mdx。
      params: {
        slug: item.entry.id === "index" ? undefined : item.entry.id,
      },
      props: { item } as SlugProps,
    }));
}

export async function GET({ props }: { props: SlugProps }) {
  const { item } = props;
  const { entry, title, description, version } = item;
  const data = (entry.data ?? {}) as Record<string, unknown>;
  const rawImage = data.socialImage;
  // 无显式 socialImage 时用每页 og 卡片，避免 config.socialImage 全站共用一张。
  const socialImage =
    typeof rawImage === "string" && rawImage.length > 0 ? rawImage : `/og/${entry.id}.png`;

  const body = [
    "---",
    `title: ${JSON.stringify(title)}`,
    ...(description ? [`description: ${JSON.stringify(description)}`] : []),
    ...(socialImage ? [`image: ${JSON.stringify(new URL(socialImage, config.site).href)}`] : []),
    ...(version ? [`version: ${JSON.stringify(version)}`] : []),
    "---",
    "",
    entry.body ?? "",
  ].join("\n");

  return utf8Text(body, "text/markdown");
}
