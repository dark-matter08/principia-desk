// Adapted from Remote Ledger. See THIRD_PARTY_NOTICES.md.
// Reference model shelf, carried over with approximate download and memory sizes.
// Capabilities describe the weights. The current tutor sends text prompts; it does
// not send image attachments or use embedding models. Web source retrieval remains
// owned by Principia Desk's teaching backend.

export type OllamaCapability = "tools" | "vision" | "reasoning" | "code" | "embedding";

export const CAPABILITY_LABEL: Record<OllamaCapability, string> = {
  tools: "Tools",
  vision: "Vision",
  reasoning: "Reasoning",
  code: "Code",
  embedding: "Embeddings",
};

export const CAPABILITY_BLURB: Record<OllamaCapability, string> = {
  tools: "Emits structured function calls, when the calling application supplies tools.",
  vision: "Reads images. Useful for diagrams or a PDF page.",
  reasoning: "Thinks step by step before answering. Slower, better on judgement calls.",
  code: "Tuned on source. Sharper when the course includes code.",
  embedding: "Turns text into vectors. Not a chat model; it cannot answer prompts.",
};

export interface OllamaModel {
  /** The exact `ollama pull` tag. */
  id: string;
  label: string;
  params: string;
  /** Download size in GB, as published. */
  sizeGb: number;
  /** Rough RAM the model wants resident, in GB. */
  ramGb: number;
  caps: OllamaCapability[];
  /** What this one is for, in Principia Desk's terms. */
  blurb: string;
}

// Curated rather than fetched: Ollama publishes no stable machine-readable library
// index, and a scraped page would rot silently. These are long-lived tags. Anything
// missing can still be pulled by name — the tab takes free text too.
export const OLLAMA_MODELS: OllamaModel[] = [
  {
    id: "llama3.2:3b",
    label: "Llama 3.2",
    params: "3B",
    sizeGb: 2.0,
    ramGb: 4,
    caps: ["tools"],
    blurb: "Compact general-purpose text model for modest hardware.",
  },
  {
    id: "qwen2.5:7b",
    label: "Qwen 2.5",
    params: "7B",
    sizeGb: 4.7,
    ramGb: 8,
    caps: ["tools", "code"],
    blurb: "General-purpose language and coding model with structured output support.",
  },
  {
    id: "qwen2.5:3b",
    label: "Qwen 2.5",
    params: "3B",
    sizeGb: 1.9,
    ramGb: 4,
    caps: ["tools", "code"],
    blurb: "Qwen's reasoning in a size that leaves room for everything else you have open.",
  },
  {
    id: "llama3.1:8b",
    label: "Llama 3.1",
    params: "8B",
    sizeGb: 4.7,
    ramGb: 8,
    caps: ["tools"],
    blurb: "Reliable, widely tested, long context. A safe pick for course explanations.",
  },
  {
    id: "mistral:7b",
    label: "Mistral",
    params: "7B",
    sizeGb: 4.1,
    ramGb: 8,
    caps: ["tools"],
    blurb: "Fast and terse. Produces concise explanations.",
  },
  {
    id: "phi3.5:3.8b",
    label: "Phi 3.5",
    params: "3.8B",
    sizeGb: 2.2,
    ramGb: 4,
    caps: [],
    blurb: "Compact text model. Try a connection test before using it for lessons.",
  },
  {
    id: "deepseek-r1:7b",
    label: "DeepSeek R1",
    params: "7B",
    sizeGb: 4.7,
    ramGb: 8,
    caps: ["reasoning"],
    blurb: "Reasons before it answers. Worth the wait for diagnostic feedback and technical explanations.",
  },
  {
    id: "deepseek-r1:1.5b",
    label: "DeepSeek R1",
    params: "1.5B",
    sizeGb: 1.1,
    ramGb: 3,
    caps: ["reasoning"],
    blurb: "Reasoning on a very small budget. Short explanations on modest hardware; limited depth.",
  },
  {
    id: "qwen2.5-coder:7b",
    label: "Qwen 2.5 Coder",
    params: "7B",
    sizeGb: 4.7,
    ramGb: 8,
    caps: ["code"],
    blurb: "Tuned for source code and programming explanations.",
  },
  {
    id: "llama3.2-vision:11b",
    label: "Llama 3.2 Vision",
    params: "11B",
    sizeGb: 7.9,
    ramGb: 12,
    caps: ["vision"],
    blurb: "Reads diagrams or a scanned course page. Needs real memory.",
  },
  {
    id: "llava:7b",
    label: "LLaVA",
    params: "7B",
    sizeGb: 4.7,
    ramGb: 8,
    caps: ["vision"],
    blurb: "The established open vision model. Image support belongs to the model; the tutor currently sends text.",
  },
  {
    id: "moondream:1.8b",
    label: "Moondream",
    params: "1.8B",
    sizeGb: 1.7,
    ramGb: 3,
    caps: ["vision"],
    blurb: "Tiny vision model. Surprisingly capable at reading text out of a screenshot.",
  },
  {
    id: "nomic-embed-text",
    label: "Nomic Embed",
    params: "137M",
    sizeGb: 0.3,
    ramGb: 1,
    caps: ["embedding"],
    blurb: "Cheap semantic search over your knowledge base. Cannot chat; pair it with one above.",
  },
];

/** Fits comfortably in the machine's memory, with room for the rest of the desktop. */
export function fitsInRam(model: OllamaModel, totalRamGb: number): boolean {
  return totalRamGb > 0 ? model.ramGb <= totalRamGb - 2 : true;
}

/** What we suggest first, given the machine. Biggest tool-capable model that fits. */
/**
 * The model to suggest for this machine.
 *
 * Not simply the biggest that fits. On a modest laptop the largest model that
 * technically fits is the one that makes the app feel broken — every explanation and drafted answer waits on it, and a first-time user reads slow
 * as broken and stops. Below the comfortable line, capability is worth less than
 * finishing.
 *
 * Above ~16 GB there is headroom for the largest that fits. Below it, the pick is the
 * best model that leaves room to actually run — roughly half the machine's memory,
 * since the OS and a browser want the rest.
 */
export function recommendedModel(totalRamGb: number): OllamaModel {
  const usable = OLLAMA_MODELS.filter(
    (m) => m.caps.includes("tools") && !m.caps.includes("embedding") && fitsInRam(m, totalRamGb)
  );
  if (!usable.length) return OLLAMA_MODELS[0];
  // sizeGb is a decent proxy for capability inside this shelf
  const bySize = [...usable].sort((a, b) => b.sizeGb - a.sizeGb);
  if (totalRamGb >= 16) return bySize[0];
  const comfortable = bySize.filter((m) => m.ramGb <= totalRamGb / 2);
  return comfortable[0] ?? bySize[bySize.length - 1];
}

/** Will this be slow enough here to be worth warning about before it is downloaded? */
export function willBeSlow(model: OllamaModel, totalRamGb: number): boolean {
  return totalRamGb > 0 && model.ramGb > totalRamGb / 2;
}

/** `llama3.2:3b` and `llama3.2:3b` from /api/tags ("llama3.2:3b") are the same thing. */
export function sameModel(a: string, b: string): boolean {
  const norm = (s: string) => String(s || "").trim().toLowerCase().replace(/:latest$/, "");
  return norm(a) === norm(b);
}

export function prettyBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 10 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

/**
 * Ollama's pull status is machine talk — "pulling 2bada8a74506" names the layer being
 * fetched, which changes several times per download and means nothing to the reader.
 * Turn it into the phase they actually care about.
 */
export function pullPhase(status: string): string {
  const s = String(status || "").toLowerCase().trim();
  if (!s) return "starting";
  if (s.includes("manifest")) return "resolving";
  if (s.startsWith("pulling")) return "downloading";
  if (s.includes("verifying")) return "verifying";
  if (s.includes("writing")) return "writing";
  if (s.includes("digest")) return "verifying";
  if (s === "success" || s === "done") return "done";
  if (s.includes("exist")) return "already here";
  return s;
}

/** Known non-chat and cloud weights do not belong in the local tutor picker. */
export function chatCandidate(name: string): boolean {
  return !name.endsWith(':cloud') && !name.endsWith('-cloud') && !OLLAMA_MODELS.some(m => sameModel(m.id, name) && m.caps.includes('embedding'));
}
