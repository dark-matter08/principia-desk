// The documentation pages, read from the repository's own Markdown at build
// time: the files under docs/, the governance files at the root, and named
// sections of the README. One commit changes the file and the page.
//
// Links inside the files are rewritten so the site stays inside itself where
// it can (a doc that has a page here links to that page) and points at the
// repository where it cannot (images, source files, everything else).
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { marked, type Tokens } from "marked";
import { getSite } from "./content";

const ROOT = resolve(process.cwd(), "..");

export interface DocSource {
  slug: string;
  title: string;
  /** One line for the index and the description meta. */
  summary: string;
  /** Where it reads from, relative to the repository root. */
  file: string;
  /** Only this `##` section of the file (with its `###` children), by heading text. */
  section?: string;
  /** Which group the index files it under. */
  group: "using" | "building" | "project";
}

export const DOCS: DocSource[] = [
  { slug: "install", title: "Install", summary: "Requirements, the installers each release carries, building from source, and what an upgrade from System Design Roulette keeps.", file: "README.md", section: "Install", group: "using" },
  { slug: "ways-out", title: "Enforcement and the ways out", summary: "The three focus policies, the five documented ways out of a locked session, and what to do if a build ever locks you out.", file: "README.md", section: "Enforcement you choose per class", group: "using" },
  { slug: "classes", title: "Your own classes", summary: "How a class of your own is built: the brief, the tutor's draft, the editor, the three checks, publishing, and what is underneath.", file: "docs/CUSTOM_CLASSES.md", group: "using" },
  { slug: "writing-rules", title: "Writing rules", summary: "The rules every generated text is held to, the tells of machine writing the desk reads for, and where each is enforced.", file: "docs/WRITING_RULES.md", group: "using" },
  { slug: "runtime", title: "The shared study runtime", summary: "Sessions, stages, preparation ahead of a study time and grading: the one runtime every lesson runs on.", file: "docs/SHARED_SESSION_RUNTIME.md", group: "building" },
  { slug: "storage", title: "Storage", summary: "The SQLite database, its numbered and checksummed migrations, and the enrollment contracts.", file: "docs/STORAGE.md", group: "building" },
  { slug: "teacher", title: "The teacher", summary: "The teacher agent and the mastery ledger that every lesson builds on.", file: "docs/TEACHER.md", group: "building" },
  { slug: "catalog", title: "The course catalogue", summary: "How a bundled course is authored: the catalogue, the curriculum, the entry map, the diagnostic bank, the reference lessons.", file: "docs/CATALOG.md", group: "building" },
  { slug: "design", title: "Design system", summary: "The page frame, the type, the three keys and the rules the interface is drawn by.", file: "DESIGN.md", group: "building" },
  { slug: "contributing", title: "Contributing", summary: "How to set up, what the gates are, how changes are proposed and reviewed.", file: "CONTRIBUTING.md", group: "project" },
  { slug: "security", title: "Security", summary: "How to report a vulnerability or a lockout bug, and what to expect back.", file: "SECURITY.md", group: "project" },
  { slug: "conduct", title: "Code of conduct", summary: "The standard everyone taking part in the project is held to.", file: "CODE_OF_CONDUCT.md", group: "project" },
];

export interface Heading {
  depth: number;
  text: string;
  id: string;
}

export interface Doc extends DocSource {
  html: string;
  headings: Heading[];
  /** The file on GitHub, for "read the source". */
  source: string;
}

const slugify = (text: string) =>
  text
    .toLowerCase()
    .replace(/<[^>]+>/g, "")
    .replace(/[^a-z0-9\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");

/** The `##` section named, with its `###` children, without its own heading. */
function section(markdown: string, heading: string): string {
  const lines = markdown.split("\n");
  const start = lines.findIndex((line) => line.trim() === `## ${heading}`);
  if (start < 0) throw new Error(`README has no section "## ${heading}"`);
  let end = lines.length;
  for (let i = start + 1; i < lines.length; i += 1) {
    if (/^## /.test(lines[i])) {
      end = i;
      break;
    }
  }
  return lines.slice(start + 1, end).join("\n");
}

/** Where a link in the repository's Markdown should go on the site. */
function rewriteHref(href: string, fromFile: string, repo: string): string {
  if (/^(https?:|mailto:|#)/.test(href)) return href;
  const [path, anchor] = href.split("#");
  // A relative path, resolved against the file's own folder.
  const dir = fromFile.includes("/") ? fromFile.slice(0, fromFile.lastIndexOf("/")) : "";
  const parts = (dir ? `${dir}/${path}` : path).split("/");
  const normalized: string[] = [];
  for (const part of parts) {
    if (part === "..") normalized.pop();
    else if (part !== "." && part !== "") normalized.push(part);
  }
  const target = normalized.join("/");
  const doc = DOCS.find((d) => d.file === target);
  if (doc) return `/docs/${doc.slug}${anchor ? `#${anchor}` : ""}`;
  if (target === "README.md") return anchor ? `${repo}#${anchor}` : "/";
  if (target === "CHANGELOG.md") return "/changelog";
  if (target === "LICENSE") return "/licence";
  if (/\.(png|jpe?g|gif|svg|webp)$/i.test(target)) {
    return `${repo.replace("https://github.com/", "https://raw.githubusercontent.com/")}/main/${target}`;
  }
  return `${repo}/blob/main/${target}${anchor ? `#${anchor}` : ""}`;
}

export function loadDoc(source: DocSource): Doc {
  const repo = getSite().repo.replace(/\/+$/, "");
  const raw = readFileSync(resolve(ROOT, source.file), "utf8");
  let markdown = source.section ? section(raw, source.section) : raw;
  // The page carries the title; a file's own first heading would repeat it.
  if (!source.section) markdown = markdown.replace(/^# .*\n+/, "");
  const headings: Heading[] = [];
  const renderer = new marked.Renderer();
  renderer.heading = ({ tokens, depth }: Tokens.Heading) => {
    const text = renderer.parser.parseInline(tokens);
    // Sections lifted from the README start at ###; the page is level two.
    const level = source.section ? Math.max(2, depth - 1) : Math.min(depth + 1, 4);
    const id = slugify(text);
    headings.push({ depth: level, text: text.replace(/<[^>]+>/g, ""), id });
    return `<h${level} id="${id}"><a class="anchor" href="#${id}">${text}</a></h${level}>\n`;
  };
  renderer.link = ({ href, title, tokens }: Tokens.Link) => {
    const text = renderer.parser.parseInline(tokens);
    const target = rewriteHref(href, source.file, repo);
    const external = /^https?:/.test(target);
    return `<a href="${target}"${title ? ` title="${title}"` : ""}${external ? ' rel="noopener"' : ""}>${text}</a>`;
  };
  renderer.image = ({ href, title, text }: Tokens.Image) => {
    const target = rewriteHref(href, source.file, repo);
    return `<figure><img src="${target}" alt="${text}" loading="lazy" />${title ? `<figcaption>${title}</figcaption>` : ""}</figure>`;
  };
  const html = marked.parse(markdown, { renderer, gfm: true, breaks: false }) as string;
  return { ...source, html, headings, source: `${repo}/blob/main/${source.file}` };
}

export function loadDocs(): Doc[] {
  return DOCS.map(loadDoc);
}
