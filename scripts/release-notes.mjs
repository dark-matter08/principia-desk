#!/usr/bin/env node
// The changelog section for one version, as plain text, for the release the
// pipeline creates and the manifest the desk's updater reads:
//
//   node scripts/release-notes.mjs 0.2.1
//
// Reads CHANGELOG.md's `## [0.2.1]` section up to the next `## `, drops the
// heading levels and the Markdown emphasis, keeps the bullets. Exits 1 when
// the version has no section, so a release cannot ship with empty notes.
import { readFileSync } from "node:fs";

const version = process.argv[2];
if (!version) {
  console.error("usage: release-notes.mjs <version>");
  process.exit(2);
}
const text = readFileSync(new URL("../CHANGELOG.md", import.meta.url), "utf8");
const lines = text.split("\n");
const start = lines.findIndex((line) => line.startsWith(`## [${version}]`));
if (start < 0) {
  console.error(`CHANGELOG.md has no section for ${version}`);
  process.exit(1);
}
let end = lines.length;
for (let i = start + 1; i < lines.length; i += 1) {
  if (lines[i].startsWith("## ")) {
    end = i;
    break;
  }
}
const body = lines
  .slice(start + 1, end)
  .map((line) =>
    line
      .replace(/^###\s+/, "")
      .replace(/\*\*([^*]+)\*\*/g, "$1")
      .replace(/`([^`]+)`/g, "$1")
      .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1"),
  )
  .join("\n")
  .replace(/\n{3,}/g, "\n\n")
  .trim();
process.stdout.write(body + "\n");
