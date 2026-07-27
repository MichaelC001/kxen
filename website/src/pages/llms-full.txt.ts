// Full-corpus markdown for AI agents — every published page in one
// document. Scope and collation live in the framework helper; reshape or
// delete this route to change the site's corpus policy.
import { renderCorpusMarkdown } from "@cloudflare/nimbus-docs";
import { homeBody, homeDescription, homeTitle } from "../lib/home-content";

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

  return new Response(`${corpus}\n\n---\n\n${home}\n`, {
    headers: { "Content-Type": "text/plain; charset=utf-8" },
  });
}
