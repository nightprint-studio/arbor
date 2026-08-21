/**
 * The suite's products — the one place that says what a product IS.
 *
 * Identity only: what it is called, what it does, and the colour that stands for it. No
 * runtime state (running? update available?) — that is resolved per render by the launcher.
 * No geometry either: the letter marks live in `product-marks.ts` and the full icons in
 * `static/products/`, both keyed by the ids declared here.
 *
 * ## Why this exists
 *
 * The accent used to live in the launcher's own roster, inside a component folder, while the
 * icons carried a second palette of their own. Two tables, no relationship — so Corvus was
 * periwinkle in the launcher and violet on its icon, and nothing would have told us. Colour
 * is identity; identity belongs in one place, and the chrome, the launcher and the artwork
 * now all read it from here.
 *
 * ## The accents
 *
 * Each is taken from the product's own icon, so the tint on a window and the mark on its
 * taskbar button are the same colour rather than two opinions about it. They come from the
 * birds, which is why they are not evenly spaced around a colour wheel — three of the seven
 * are warm. That is a real limit and worth knowing: at the low opacity the window tint uses,
 * Merula's orange, Bennu's crimson and Tyto's gold are *adjacent*, not distinct. The tint is
 * ambience, and the icon beside it is what actually names the product.
 */

/** Every product of the suite. */
export type ProductId = 'corvus' | 'merula' | 'sitta' | 'bennu' | 'picus' | 'tyto' | 'garrulus';

export interface ProductIdentity {
  id: ProductId;
  name: string;
  /** The bird the name comes from — shown in the launcher's detail card. */
  bird: string;
  /** One-line label, for tight spots (the tree's footer, a tooltip). */
  role: string;
  /** The colour that stands for the product, taken from its icon. */
  accent: string;
  /** A sentence or two on what it actually does — the welcome page has the room for it,
   *  and "Git & CI client" tells a newcomer nothing. */
  blurb?: string;
}

/**
 * The roster, in the order the launcher lays its branches out.
 *
 * Adding a product means: an entry here, an SVG in `design/icons/<id>/`, a run of
 * `design/icons/rasterize.ps1`, and a mark in `product-marks.ts`.
 */
export const PRODUCTS: ProductIdentity[] = [
  {
    id: 'corvus', name: 'Corvus', bird: 'crow', role: 'Git & CI client', accent: '#a855f7',
    blurb: 'Branches, commits and diffs, merge requests, pipelines and issues — the whole day-to-day of a repository in one window.',
  },
  {
    id: 'merula', name: 'Merula', bird: 'blackbird', role: 'Music synthesizer', accent: '#ff8a1f',
    blurb: 'Live-coding DAW: write patterns in a text DSL and hear them immediately, with sample banks, a mixer and audio export.',
  },
  {
    // `nuthatch`, not `treecreeper`: Sitta IS the nuthatch genus — a treecreeper is Certhia,
    // a different bird that climbs the other way up. The icon is drawn on the nuthatch.
    id: 'sitta', name: 'Sitta', bird: 'nuthatch', role: 'File explorer', accent: '#8ecae6',
    blurb: 'A file manager that knows about git: status overlays while you browse, plus preview, search and the usual file operations.',
  },
  {
    id: 'tyto', name: 'Tyto', bird: 'barn owl', role: 'Screen recorder', accent: '#e0a33c',
    blurb: 'Screen capture from the tray: record a region or a window, or grab a still, without hunting for the app first.',
  },
  {
    // Crimson rather than the orange the B is drawn in: Merula is already orange, and at the
    // opacity a window tint runs at the two were the same colour. Crimson is the root of the
    // same gradient, so it stays faithful to the icon.
    id: 'bennu', name: 'Bennu', bird: 'firebird', role: 'Java & Rust editor', accent: '#e03131',
    blurb: 'Editor and semantic engine for two stacks: legacy Java — Struts, Spring XML, MyBatis, JSP — and Rust, with Cargo, a debugger and a Bevy toolchain of its own. Navigation, live validation, refactors and run configurations on both.',
  },
  {
    id: 'picus', name: 'Picus', bird: 'woodpecker', role: 'SQL studio', accent: '#69db7c',
    blurb: 'Oracle and PostgreSQL client, and maintainer of the SQL scripts they install from: it keeps the two dialect branches in step and generates the changes that keep them there.',
  },
  {
    // The jay's wing-flash blue, not the rose of its body: the rose sat next to Tyto's gold,
    // and this blue is deep enough to read apart from Sitta's pale slate.
    // The jay caches acorns in a thousand places and remembers every one — the bird the
    // product is named for, and the behaviour it is for.
    id: 'garrulus', name: 'Garrulus', bird: 'jay', role: 'Note e appunti', accent: '#3b8ee0',
    blurb: 'A vault of plain markdown notes — links, tags, tasks and templates per note kind — that keeps itself in step across your machines. The files stay yours: any other markdown editor opens the same folder.',
  },
];

const BY_ID = new Map<string, ProductIdentity>(PRODUCTS.map((p) => [p.id, p]));

/** Whether `id` names a product of the suite. */
export function isProductId(id: string): id is ProductId {
  return BY_ID.has(id);
}

/** A product's identity, or `null` for anything else — `home`, the launcher, a window label
 *  that is not a product. Callers use the `null` to mean "this is Arbor, not a product". */
export function productIdentity(id: string): ProductIdentity | null {
  return BY_ID.get(id) ?? null;
}

/** A product's accent, or `null`. */
export function productAccent(id: string): string | null {
  return BY_ID.get(id)?.accent ?? null;
}

/**
 * The product a Tauri window label belongs to, or `null` when the window is Arbor's own
 * (the launcher, the tabbed container) or a chromeless overlay.
 *
 * Labels carry a `-N` suffix for second instances (`bennu-2`), and one of them predates its
 * product's name: the file explorer's window is still labelled `explorer`, and it is Sitta.
 */
export function productForWindowLabel(label: string): ProductId | null {
  // The recording HUD is a chromeless overlay — no title bar, no rail, nothing to tint.
  if (label === 'tyto-hud') return null;
  const base = label.split('-')[0];
  if (base === 'explorer') return 'sitta';
  return isProductId(base) ? base : null;
}

/**
 * The product the user is currently looking at.
 *
 * In the tabbed container that is the active tab; in a standalone product window it is the
 * window's own label. Used when an action has to be recorded **for a product** — installing a
 * package, most of all, because installing from Corvus must not put it in Bennu's palette.
 *
 * `null` for the launcher and the welcome tab: neither hosts plugins, so neither is somewhere
 * a package can be installed for.
 */
export function currentProduct(
  windowLabel: string,
  inContainer: boolean,
  activeSurface: string | null,
): ProductId | null {
  if (inContainer) {
    if (!activeSurface || activeSurface === 'home') return null;
    return isProductId(activeSurface) ? activeSurface : null;
  }
  return productForWindowLabel(windowLabel);
}
