import { homeSource } from "../lib/home-content";
import { utf8Text } from "../lib/text-response";

export const prerender = true;

export function GET() {
  return utf8Text(homeSource, "text/markdown");
}
