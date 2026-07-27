import { homeSource } from "../lib/home-content";

export const prerender = true;

export function GET() {
  return new Response(homeSource, {
    headers: { "Content-Type": "text/markdown; charset=utf-8" },
  });
}
