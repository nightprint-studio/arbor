/**
 * Which taglib prefixes a JSP **declares**, and the colour each one wears.
 *
 * A legacy page opens with a stack of near-identical directives —
 * `<%@ taglib prefix="s" uri="/struts-tags"%>`, `prefix="wp"`, `prefix="c"` — and
 * below them a thousand lines of `<s:…>`, `<wp:…>`, `<c:…>` that a single "taglib
 * tag" colour renders indistinguishable. Struts, Entando and JSTL tags are three
 * different languages with three different manuals; telling them apart is the first
 * thing you do reading the page, and the parser already knows the answer.
 *
 * So: **each declared prefix gets a hue, and its declaration wears the same one.**
 * The directive at the top is the legend for everything under it.
 *
 * Two properties matter, and they pull against each other:
 *
 *  - *stable across files* — `s:` should be the same colour in all 400 JSPs, so the
 *    hue comes from a hash of the prefix name ({@link namespaceSlotFor}), not from
 *    the order the page happens to declare them in;
 *  - *distinct within a file* — two prefixes hashing to the same slot would recreate
 *    exactly the problem this solves, so a collision moves the second one to the next
 *    free slot.
 *
 * Only a prefix this document declares gets a hue. An undeclared one (a typo, a
 * copied fragment whose directive was left behind) keeps the plain tag colour — which
 * is a quiet, free hint that the page will not render it either.
 */

import {
  NAMESPACE_SLOTS,
  namespaceSlotFor,
  type Node,
  type Tree,
} from '$lib/components/shared/ui/code-editor';

/** The three shapes a tag takes in the grammar. All flat, all root children. */
const TAG_TYPES = new Set(['start_tag', 'self_closing_tag', 'end_tag']);

/**
 * A `<%@ taglib … prefix="x" … %>` directive, in either attribute order (`uri` first
 * is the common spelling, `prefix` first is legal and does occur). Anchored at `<%@`
 * with `taglib` as the directive name, so `<%@ page %>` / `<%@ include %>` — which
 * declare no prefix and stay the ordinary directive colour — never match.
 */
const TAGLIB_DIRECTIVE = /^<%@\s*taglib\b[\s\S]*?\bprefix\s*=\s*(["'])([^"']+)\1/;

/** Prefix → palette slot, computed once per parse tree. A new tree per reparse, so an
 *  edited directive re-colours its tags on the next keystroke; the old tree's entry
 *  goes with the tree. */
const BY_TREE = new WeakMap<Tree, Map<string, number>>();

/**
 * The slot for a `tag_name` leaf (`s:iterator`), or `undefined` if it carries no
 * prefix or the page never declared it.
 *
 * Reading `node.text` is what costs here — it is called for every tag in the file on
 * every reparse — so a page with no taglibs at all (plain HTML in a `.jsp`) answers
 * from the empty map without touching the text.
 */
export function tagSlot(node: Node): number | undefined {
  const slots = slotsOf(node.tree);
  if (slots.size === 0) return undefined;
  const name = node.text;
  const cut = name.indexOf(':');
  return cut > 0 ? slots.get(name.slice(0, cut)) : undefined;
}

/** The slot for a `jsp_directive` leaf, or `undefined` if it is not a `taglib` one. */
export function directiveSlot(node: Node): number | undefined {
  const prefix = TAGLIB_DIRECTIVE.exec(node.text)?.[2];
  return prefix === undefined ? undefined : slotsOf(node.tree).get(prefix);
}

function slotsOf(tree: Tree): Map<string, number> {
  let slots = BY_TREE.get(tree);
  if (!slots) {
    slots = collect(tree);
    BY_TREE.set(tree, slots);
  }
  return slots;
}

/**
 * One pass over the tree's **top-level** children — a strict subset of the walk the
 * highlighter already does to build its decorations, and the grammar is flat, so both
 * the directives and the tags are root children wherever they sit in the page.
 *
 * ## Colours go to the libraries the page uses
 *
 * A legacy page declares far more libraries than it writes: eight `<%@ taglib %>` lines
 * at the top and three prefixes actually used below. Handing out slots in declaration
 * order spends the palette on libraries nothing on the page wears, and the ones that
 * *are* worn end up sharing — which is the whole problem, arrived at by being fair to
 * declarations nobody reads.
 *
 * So the used prefixes are served first, and the unused declarations take what is left.
 * A page that declares more than the palette holds leaves its dead declarations the
 * ordinary directive colour, which is a fair thing for a dead declaration to look like.
 */
function collect(tree: Tree): Map<string, number> {
  const declared: string[] = [];
  const used = new Set<string>();
  const root = tree.rootNode;
  for (let i = 0; i < root.childCount; i++) {
    const child = root.child(i);
    if (!child) continue;
    if (child.type === 'jsp_directive') {
      const prefix = TAGLIB_DIRECTIVE.exec(child.text)?.[2];
      // First declaration wins: a page that declares one prefix twice (an include brought
      // it in as well) gets one colour, not a second slot burned on a duplicate.
      if (prefix !== undefined && !declared.includes(prefix)) declared.push(prefix);
    } else if (TAG_TYPES.has(child.type)) {
      const name = tagNameOf(child);
      const cut = name.indexOf(':');
      if (cut > 0) used.add(name.slice(0, cut));
    }
  }

  const slots = new Map<string, number>();
  const taken = new Set<number>();
  for (const prefix of declared) {
    if (used.has(prefix)) slots.set(prefix, claim(prefix, taken));
  }
  for (const prefix of declared) {
    // Only while there is a colour nobody is using: a declaration nothing on the page
    // wears must never take the hue off one that is worn.
    if (!used.has(prefix) && taken.size < NAMESPACE_SLOTS) {
      slots.set(prefix, claim(prefix, taken));
    }
  }
  return slots;
}

/** The `tag_name` child of a tag node, or `""`. Reads the NAME rather than the tag's whole
 *  text: a tag can be a hundred characters and only its first word is being asked about. */
function tagNameOf(tag: Node): string {
  for (let i = 0; i < tag.childCount; i++) {
    const child = tag.child(i);
    if (child?.type === 'tag_name') return child.text;
  }
  return '';
}

/** The prefix's preferred slot, or the next free one if that is already spoken for.
 *  Past {@link NAMESPACE_SLOTS} declarations the hues necessarily repeat — a page with
 *  seven taglibs has a genuine ambiguity, and inventing a seventh near-identical shade
 *  would only make it harder to see. */
function claim(prefix: string, taken: Set<number>): number {
  const preferred = namespaceSlotFor(prefix);
  for (let step = 0; step < NAMESPACE_SLOTS; step++) {
    const slot = (preferred + step) % NAMESPACE_SLOTS;
    if (!taken.has(slot)) {
      taken.add(slot);
      return slot;
    }
  }
  return preferred;
}
