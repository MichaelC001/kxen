const UTF8_BOM = "\uFEFF";

export function utf8Text(body: string, mediaType: "text/markdown" | "text/plain"): Response {
  return new Response(`${UTF8_BOM}${body}`, {
    headers: { "Content-Type": `${mediaType}; charset=utf-8` },
  });
}
