import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { siteSchema, type Site } from "./schema";

// Where the page's content comes from: the committed JSON, parsed against the
// schema so a broken edit fails the build rather than rendering a half page.
// The swap point for a CMS later is this module alone.
const LOCAL = resolve(process.cwd(), "src/content/site.json");
let cached: Site | null = null;

export function getSite(): Site {
  if (cached) return cached;
  cached = siteSchema.parse(JSON.parse(readFileSync(LOCAL, "utf8")));
  return cached;
}
