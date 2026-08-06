/**
 * A categorical palette for **namespaced** tokens — a JSP taglib prefix, an XML
 * namespace, anything whose interesting property is *which family it belongs to*
 * rather than *what kind of thing it is*.
 *
 * The rest of the token palette is semantic: a keyword is orange because it is a
 * keyword, everywhere, in every language. This one is the opposite — the colour
 * carries no meaning of its own, it only has to be *different from its neighbours*
 * and *the same as its declaration*. So the hues here are deliberately not tied to
 * any `--syntax-*` role, and a language assigns them by name (see
 * {@link namespaceSlotFor}) rather than by node type.
 *
 * ## Which hues are left
 *
 * Fewer than it looks. On the line these colours actually appear on —
 * `<s:if test="%{…}">` — the neighbours are already spoken for: **orange** and
 * **gold** (keywords, plain HTML tags), **olive** (scriptlets and directives),
 * **violet** (attribute names), and above all **green**, which every attribute value
 * in the file is wearing. A tag name has a string sitting two characters to its right
 * on nearly every line, so a namespace hue anywhere in the green band reads as a
 * second shade of the same thing — which is exactly what the first version of this
 * palette got wrong with its teal.
 *
 * So the hues live in the two clear bands, blue-to-indigo and magenta-to-rose, and
 * are bright rather than muted: the muted end of both is where `--syntax-field`'s
 * violet already sits.
 */

/**
 * The slots: **four hues, then two of them lifted**.
 *
 * Two bands do not hold six well-separated hues — squeezing six in produces pairs
 * nobody tells apart at 13px, which is this palette's own failure mode reached from
 * the other side. But four slots is not enough either: a legacy JSP routinely opens
 * with five or six `<%@ taglib %>` lines, and a palette that runs out repeats a colour
 * for two different libraries, which is the exact complaint it was built to answer.
 *
 * So past the four hues the palette changes **weight** rather than inventing a fifth
 * hue: slots 4 and 5 are the azure and the violet, much lighter. A light cyan beside a
 * mid azure is a distinction the eye makes easily, and it reads as deliberate rather
 * than as two shades of a colour that could not decide.
 *
 * They sit last on purpose: a collision walks *forward* from the hash's preference, so
 * the four hues are handed out first and the lifted pair is what a fifth and sixth
 * library get.
 */
export const NAMESPACE_COLORS = [
  'var(--syntax-ns-0, #4fb8f5)', // azure
  'var(--syntax-ns-1, #a78bfa)', // violet — brighter and bluer than `--syntax-field`
  'var(--syntax-ns-2, #e879c8)', // magenta
  'var(--syntax-ns-3, #f0808a)', // rose
  'var(--syntax-ns-4, #6ee0f0)', // cyan — the azure, lifted
  'var(--syntax-ns-5, #cbb2ff)', // lavender — the violet, lifted
];

/** How many distinct namespace hues exist. */
export const NAMESPACE_SLOTS = NAMESPACE_COLORS.length;

/**
 * The slot a namespace *prefers*, derived from its name (FNV-1a) so that `s:` reads
 * the same colour in every file of a project and across restarts — the alternative,
 * numbering them in the order the document declares them, repaints a prefix
 * differently in every file that happens to list its taglibs in another order.
 *
 * A caller that needs the slots within one document to be *distinct* resolves
 * collisions itself (the preference is a starting point, not a promise).
 */
export function namespaceSlotFor(name: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0) % NAMESPACE_SLOTS;
}

/** The token class for a slot — what a `classify` returns, styled by the editor theme. */
export function namespaceTokenClass(slot: number): string {
  return `ns-${((slot % NAMESPACE_SLOTS) + NAMESPACE_SLOTS) % NAMESPACE_SLOTS}`;
}

/** The `.cm-tok-ns-*` rules, generated from the palette so colours and classes cannot
 *  drift apart. Spread into the editor theme. */
export const namespaceThemeSpec: Record<string, { color: string }> =
  Object.fromEntries(NAMESPACE_COLORS.map((color, i) => [`.cm-tok-ns-${i}`, { color }]));
