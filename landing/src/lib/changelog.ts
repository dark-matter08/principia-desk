import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// The changelog, read from the repository at build time. The landing folder
// lives inside the app's repository, so a release and its notes are one
// commit and this page is a reader, not a second copy. Deliberately no
// markdown library: the file uses six constructs.
const FILE = resolve(process.cwd(), "../CHANGELOG.md");

export interface Block {
  kind: "para" | "list" | "sub";
  text?: string;
  items?: string[];
}
export interface Release {
  anchor: string;
  heading: string;
  blocks: Block[];
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** Bold, inline code and links: the three things the changelog uses inline. */
export function inline(s: string): string {
  return escapeHtml(s)
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\[([^\]]+)\]\((https?:[^)]+)\)/g, '<a href="$2" rel="noopener">$1</a>');
}

export function releases(): Release[] {
  const raw = readFileSync(FILE, "utf8");
  const out: Release[] = [];
  let current: Release | null = null;
  let list: string[] | null = null;
  const flushList = () => {
    if (current && list && list.length) current.blocks.push({ kind: "list", items: list });
    list = null;
  };
  for (const line of raw.split("\n")) {
    if (line.startsWith("## ")) {
      flushList();
      const heading = line.slice(3).trim();
      if (heading === "Changelog") continue;
      current = { anchor: heading.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, ""), heading, blocks: [] };
      out.push(current);
      continue;
    }
    if (!current) continue;
    if (line.startsWith("### ")) { flushList(); current.blocks.push({ kind: "sub", text: line.slice(4).trim() }); continue; }
    if (/^\s*-\s+/.test(line)) { (list ??= []).push(line.replace(/^\s*-\s+/, "")); continue; }
    if (/^\[.+\]:\s+https?:/.test(line)) continue; // reference links at the foot
    if (line.trim() === "") { flushList(); continue; }
    flushList();
    current.blocks.push({ kind: "para", text: line.trim() });
  }
  flushList();
  return out;
}
