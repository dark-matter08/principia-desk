// The newest release, read from GitHub while the page is built, so the install
// section can name the version and link each platform's installer directly. A
// build-time value is right on the day it is built; the page asks again in the
// reader's browser and corrects itself. A failure here never fails the build:
// the section falls back to the releases page.
import { getSite } from "./content";

export type AssetKind = "macos-aarch64" | "macos-x86_64" | "windows-msi" | "windows-exe" | "linux-deb" | "linux-rpm" | "linux-appimage";

export interface ReleaseAsset {
  kind: AssetKind;
  name: string;
  url: string;
  bytes: number;
}

export interface Release {
  tag: string;
  url: string;
  publishedAt: string | null;
  assets: ReleaseAsset[];
}

/** Which installer a release asset is, from its name. */
export function kindOf(name: string): AssetKind | null {
  const n = name.toLowerCase();
  if (n.endsWith(".dmg")) return n.includes("aarch64") || n.includes("arm64") ? "macos-aarch64" : "macos-x86_64";
  if (n.endsWith(".msi")) return "windows-msi";
  if (n.endsWith(".exe")) return "windows-exe";
  if (n.endsWith(".deb")) return "linux-deb";
  if (n.endsWith(".rpm")) return "linux-rpm";
  if (n.endsWith(".appimage")) return "linux-appimage";
  return null;
}

export function apiLatest(): string {
  return getSite().repo.replace(/\/+$/, "").replace("https://github.com/", "https://api.github.com/repos/") + "/releases/latest";
}

export async function latestRelease(): Promise<Release | null> {
  try {
    const response = await fetch(apiLatest(), {
      headers: { accept: "application/vnd.github+json", "user-agent": "principia-landing" },
      signal: AbortSignal.timeout(8000),
    });
    if (!response.ok) return null;
    const data = (await response.json()) as { tag_name?: string; html_url?: string; published_at?: string; assets?: { name: string; browser_download_url: string; size: number }[] };
    if (!data.tag_name) return null;
    return {
      tag: data.tag_name,
      url: data.html_url ?? getSite().releases,
      publishedAt: data.published_at ?? null,
      assets: (data.assets ?? [])
        .map((a) => ({ kind: kindOf(a.name), name: a.name, url: a.browser_download_url, bytes: a.size }))
        .filter((a): a is ReleaseAsset => a.kind !== null),
    };
  } catch {
    return null;
  }
}

export function megabytes(bytes: number): string {
  return `${(bytes / 1048576).toFixed(bytes > 50 * 1048576 ? 0 : 1)} MB`;
}
