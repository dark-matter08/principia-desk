/**
 * Mermaid diagrams as pictures a PDF can hold.
 *
 * The reader renders diagrams as SVG with HTML labels, which a PDF cannot
 * embed and a canvas refuses to rasterize. For the PDF each diagram is
 * rendered again with plain SVG text, a light theme and a system font, then
 * drawn on a canvas at print resolution. A diagram that will not render
 * yields nothing, and the PDF shows its source instead.
 */

let mermaidModule: Promise<typeof import('mermaid')> | null = null;
function loadMermaid() {
  if (!mermaidModule) {
    mermaidModule = import('mermaid').then((mod) => {
      mod.default.initialize({ startOnLoad: false, securityLevel: 'strict', theme: 'dark', fontFamily: 'inherit' });
      return mod;
    });
  }
  return mermaidModule;
}

const PRINT_DIRECTIVE =
  '%%{init: {"theme": "neutral", "fontFamily": "Helvetica, Arial, sans-serif", "htmlLabels": false, "flowchart": {"htmlLabels": false}, "sequence": {"useMaxWidth": false}}}%%\n';

export interface DiagramImage {
  /** PNG data URL. */
  dataUrl: string;
  /** Natural size in CSS pixels, before print scaling. */
  width: number;
  height: number;
}

/** Render one diagram source to a PNG data URL, or null if it cannot be drawn. */
export async function renderDiagramImage(source: string, scale = 2): Promise<DiagramImage | null> {
  if (typeof document === 'undefined') return null;
  let svg: string;
  try {
    const mermaid = (await loadMermaid()).default;
    const id = `pdf-mmd-${Math.random().toString(36).slice(2)}`;
    ({ svg } = await mermaid.render(id, PRINT_DIRECTIVE + source.trim()));
  } catch {
    return null;
  }
  const size = measure(svg);
  if (!size) return null;
  // An <img> renders the SVG with the page's fonts unavailable; a system
  // font is named so labels keep their metrics.
  const prepared = svg
    .replace(/font-family:\s*inherit/g, 'font-family: Helvetica, Arial, sans-serif')
    .replace(/<svg([^>]*?)\swidth="[^"]*"/, '<svg$1')
    .replace(/<svg([^>]*?)\sheight="[^"]*"/, '<svg$1')
    .replace(/<svg/, `<svg width="${size.width}" height="${size.height}"`);
  const image = new Image();
  image.decoding = 'async';
  const loaded = new Promise<boolean>((resolve) => {
    image.onload = () => resolve(true);
    image.onerror = () => resolve(false);
  });
  image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(prepared)}`;
  if (!(await loaded)) return null;
  const canvas = document.createElement('canvas');
  canvas.width = Math.ceil(size.width * scale);
  canvas.height = Math.ceil(size.height * scale);
  const context = canvas.getContext('2d');
  if (!context) return null;
  context.fillStyle = '#ffffff';
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.scale(scale, scale);
  try {
    context.drawImage(image, 0, 0, size.width, size.height);
    return { dataUrl: canvas.toDataURL('image/png'), width: size.width, height: size.height };
  } catch {
    return null;
  }
}

/** The diagram's natural size, from its viewBox or its width and height. */
function measure(svg: string): { width: number; height: number } | null {
  const viewBox = svg.match(/viewBox="([\d.\-]+)\s+([\d.\-]+)\s+([\d.]+)\s+([\d.]+)"/);
  if (viewBox) {
    const width = Number(viewBox[3]);
    const height = Number(viewBox[4]);
    if (width > 0 && height > 0) return { width, height };
  }
  const width = Number(svg.match(/\swidth="([\d.]+)/)?.[1]);
  const height = Number(svg.match(/\sheight="([\d.]+)/)?.[1]);
  return width > 0 && height > 0 ? { width, height } : null;
}
