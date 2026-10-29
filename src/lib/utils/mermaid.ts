/**
 * Mermaid, loaded on first sight and themed with the app's colours.
 *
 * Mermaid is by a distance the largest thing this app could depend on, and a session without a diagram
 * must not pay for it: the `import()` below is a split point, so its bundle is fetched from disk the first
 * time something asks for a diagram and never before. Everything that draws one — the Markdown editor's
 * live preview, the assistant's answers — goes through here, so the library is initialised once, the
 * theme is read in one place, and a diagram drawn twice is drawn once.
 */

/** The largest source drawn. Past this a diagram is a wall of text on screen long before it is a picture. */
export const MERMAID_MAX_SOURCE = 20_000;

/** How many drawn pictures are kept; the oldest is dropped first. */
const CACHE_LIMIT = 48;

type MermaidApi = typeof import('mermaid').default;

/** The library, initialised once. `null` until something asks for it. */
let load: Promise<MermaidApi> | null = null;
/** The theme the library was initialised for — the value of one CSS variable is enough of a fingerprint,
 *  and it is what changes when a theme is switched. */
let loadedTheme = '';
/** Rendered SVG by source, so scrolling past a diagram (or re-rendering the text around it) does not
 *  re-lay it out. */
const cache = new Map<string, string>();
let seq = 0;

/** The Arbor colours mermaid should draw with, read from the live theme. */
function themeVariables(): Record<string, string> {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
  const accent = v('--accent', '#3d7fff');
  const bg = v('--bg-base', '#1e1f22');
  const text = v('--text-primary', '#dfe1e5');
  return {
    background: bg,
    primaryColor: v('--bg-elevated', '#2b2d31'),
    primaryTextColor: text,
    primaryBorderColor: accent,
    secondaryColor: v('--bg-overlay', '#33353a'),
    tertiaryColor: v('--bg-hover', '#3a3d42'),
    lineColor: v('--text-muted', '#7a7d85'),
    textColor: text,
    mainBkg: v('--bg-elevated', '#2b2d31'),
    nodeBorder: accent,
    clusterBkg: v('--bg-overlay', '#33353a'),
    clusterBorder: v('--border', '#404348'),
    titleColor: text,
    edgeLabelBackground: bg,
    fontFamily: v('--font-ui-sans', 'system-ui, sans-serif'),
  };
}

export async function mermaidLib(): Promise<MermaidApi> {
  const theme = getComputedStyle(document.documentElement).getPropertyValue('--bg-base').trim();
  if (load && theme !== loadedTheme) {
    // The theme moved under us: re-initialise and drop the pictures drawn in the old colours.
    load = null;
    cache.clear();
  }
  if (!load) {
    loadedTheme = theme;
    load = import('mermaid').then((m) => {
      m.default.initialize({
        startOnLoad: false,
        // `strict` sanitises the HTML a diagram's labels may contain. A diagram is something you are
        // given (a file, a model's answer) as often as something you wrote.
        securityLevel: 'strict',
        theme: 'base',
        themeVariables: themeVariables(),
      });
      return m.default;
    });
    // A failed import (disk, a stale chunk) must be retried by the next diagram, not remembered.
    load.catch(() => { load = null; });
  }
  return load;
}

/** The message of whatever was thrown: mermaid's own, which names the line it could not parse and is
 *  the only useful thing to show for a diagram that is wrong. */
export function mermaidErrorMessage(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  return raw.trim() || 'The diagram could not be drawn.';
}

/** The picture of a source already drawn, or `null` — synchronous, for text that re-renders around it. */
export function cachedMermaid(code: string): string | null {
  return cache.get(code) ?? null;
}

/** Throws (with mermaid's message, line included) unless the source is a diagram mermaid understands. */
export async function parseMermaid(code: string): Promise<void> {
  if (!code.trim()) throw new Error('The diagram is empty.');
  if (code.length > MERMAID_MAX_SOURCE) throw new Error(`The diagram is too large (${code.length} characters, at most ${MERMAID_MAX_SOURCE}). Split it or leave detail out.`);
  const mermaid = await mermaidLib();
  await mermaid.parse(code);
}

/** The SVG for one diagram, or a thrown error carrying mermaid's own message. */
export async function renderMermaid(code: string): Promise<string> {
  const hit = cache.get(code);
  if (hit) return hit;
  await parseMermaid(code);
  const mermaid = await mermaidLib();
  seq += 1;
  const id = `arbor-mmd-${seq}`;
  try {
    const { svg } = await mermaid.render(id, code);
    cache.delete(code);
    cache.set(code, svg);
    if (cache.size > CACHE_LIMIT) cache.delete(cache.keys().next().value as string);
    return svg;
  } catch (e) {
    // A render that fails leaves its scratch element (and sometimes an error picture) in the page.
    document.getElementById(id)?.remove();
    document.getElementById(`d${id}`)?.remove();
    throw e;
  }
}

export type MermaidVerdict = { ok: true } | { ok: false; error: string };

/** Whether a source can be drawn — parsed and rendered once, the picture kept for when it is shown. */
export async function validateMermaid(code: string): Promise<MermaidVerdict> {
  try {
    await renderMermaid(code);
    return { ok: true };
  } catch (e) {
    return { ok: false, error: mermaidErrorMessage(e) };
  }
}
