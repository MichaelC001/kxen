import { renderCorpusMarkdown } from "@cloudflare/nimbus-docs";
import { homeBody, homeDescription, homeTitle } from "../lib/home-content";
import { utf8Text } from "../lib/text-response";

export const prerender = true;

export async function GET() {
  const corpus = await renderCorpusMarkdown();
  const home = [
    `# ${homeTitle}`,
    "",
    homeDescription,
    "",
    "Source: https://kxen.ai/ · Markdown: https://kxen.ai/index.md",
    "",
    homeBody,
  ].join("\n");

  return utf8Text(`${corpus}\n\n---\n\n${home}\n`, "text/plain");
}
