/**
 * Colours for the **sections** of a `pom.xml`.
 *
 * A pom is one long document in which a handful of top-level blocks mean very different things:
 * what the project depends on, what it only *manages* versions for, how it builds, what a
 * profile switches on. In stock XML colouring every tag is the same tag colour, so the eye has
 * nothing to find "where does `<dependencies>` end and `<build>` begin" with. Here every tag
 * name inside a section takes that section's hue — the section's own tag and everything nested
 * in it, closing tags included — so the structure is visible at a glance and a plugin's
 * `<dependencies>` is not mistaken for the project's.
 *
 * The project coordinates (`groupId`, `artifactId`, `version`, …) are left alone on purpose:
 * they are the header, and the loudest thing in a file should not be its name.
 *
 * This colours **tag names only**. The values between them (names, versions, scopes, properties)
 * keep the semantic colours the backend's pom reading gives them (`.cm-fw-maven-*` in
 * `BennuEditor.svelte`) — the two layers answer different questions, "which block am I in" and
 * "what is this text", and neither touches what the other paints.
 *
 * ## Why a scan and not the parser
 *
 * The XML mode is a stream parser: it has no tree, so nothing says which element a tag sits in.
 * A single regex pass keeps a stack of open names — comments, CDATA and processing instructions
 * skipped — which is exactly the depth information needed and nothing more. It is bounded by
 * {@link MAX_SCANNED}: a generated multi-megabyte "pom" is coloured the ordinary way rather
 * than costing a keystroke its frame.
 *
 * The hues are the namespace palette's (`--syntax-ns-*`): they are the ones left once the
 * semantic token colours are spoken for, and that is also why they are applied with
 * `!important` — the tag-name token inside the mark carries its own colour.
 */

import { RangeSetBuilder, type Extension } from '@codemirror/state';
import {
  Decoration,
  EditorView,
  ViewPlugin,
  type DecorationSet,
  type ViewUpdate,
} from '@codemirror/view';
import { NAMESPACE_COLORS } from '$lib/components/shared/ui/code-editor';

/** Past this many characters the file is left to the plain XML colouring. */
const MAX_SCANNED = 600_000;

/** A section's colour group → palette slot. Two groups may share a hue only when they are the
 *  same idea twice (`parent` and `modules` are both "other projects"). */
const GROUP_SLOT: Record<string, number> = {
  deps: 0,      // azure — what the project uses
  depmgmt: 4,   // cyan — the same idea, lifted: versions managed, nothing pulled in
  props: 1,     // violet — values the rest of the file refers to
  build: 2,     // magenta — how it is built
  profiles: 3,  // rose — what a switch turns on
  repos: 3,     // rose — where things come from
  projects: 5,  // lavender — the other projects: parent and modules
};

/** The top-level element → its colour group. Anything not listed (the coordinates, `name`,
 *  `description`, `licenses`, …) is not coloured. */
const SECTION_GROUP: Record<string, string> = {
  dependencies: 'deps',
  dependencyManagement: 'depmgmt',
  properties: 'props',
  build: 'build',
  reporting: 'build',
  profiles: 'profiles',
  repositories: 'repos',
  pluginRepositories: 'repos',
  distributionManagement: 'repos',
  parent: 'projects',
  modules: 'projects',
};

/** Comments, CDATA and processing instructions (skipped) or one tag: `[1]` is `/` for a closing
 *  tag, `[2]` the name, `[4]` is `/` for a self-closing one. Quoted attribute values may hold `>`. */
const TOKEN =
  /<!--[\s\S]*?-->|<!\[CDATA\[[\s\S]*?\]\]>|<\?[\s\S]*?\?>|<(\/?)([A-Za-z_][\w.:-]*)((?:"[^"]*"|'[^']*'|[^>"'])*?)(\/?)>/g;

const MARKS: Record<string, Decoration> = Object.fromEntries(
  Object.keys(GROUP_SLOT).map((group) => [group, Decoration.mark({ class: `cm-pom-${group}` })]),
);

/** The tag-name decorations of a pom's text, in document order (a `RangeSetBuilder` requires it). */
function scan(text: string): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  if (text.length > MAX_SCANNED) return builder.finish();

  const open: string[] = []; // element names; `open[0]` is the root (`project`), `open[1]` a section
  TOKEN.lastIndex = 0;
  for (let m = TOKEN.exec(text); m; m = TOKEN.exec(text)) {
    const name = m[2];
    if (!name) continue; // a comment, CDATA or processing instruction
    const closing = m[1] === '/';
    const selfClosing = m[4] === '/';

    // The section is the root's child this tag sits in, or is. A tag of the root itself has none.
    const section = closing
      ? open[1]
      : open.length === 1 ? name : open[1];
    const group = section ? SECTION_GROUP[section] : undefined;
    if (group) {
      const from = m.index + (closing ? 2 : 1);
      builder.add(from, from + name.length, MARKS[group]);
    }

    if (closing) open.pop();
    else if (!selfClosing) open.push(name);
  }
  return builder.finish();
}

const pomSectionPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = scan(view.state.doc.toString());
    }
    update(update: ViewUpdate) {
      if (update.docChanged) this.decorations = scan(update.state.doc.toString());
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

/**
 * How much of a group's hue is kept, the rest being the editor background.
 *
 * Only the groups whose palette colour is too bright to sit beside the theme's own tags: the
 * lifted lavender (`projects`) and the full magenta (`build`) read as the loudest thing on the page
 * on a dark theme. Mixing with the background lowers the lightness and keeps the hue, which is why
 * it is a mix and not a different colour — the section stays recognisable as the same family.
 */
const KEEP_PERCENT: Record<string, number> = {
  projects: 62,
  build: 74,
};

const sectionColour = (group: string, slot: number): string => {
  const keep = KEEP_PERCENT[group];
  return keep === undefined
    ? NAMESPACE_COLORS[slot]
    : `color-mix(in srgb, ${NAMESPACE_COLORS[slot]} ${keep}%, var(--bg-base, #0d1117))`;
};

/** `!important` because the tag-name token the mark wraps (or sits inside) paints its own colour. */
const pomSectionTheme = EditorView.baseTheme(
  Object.fromEntries(
    Object.entries(GROUP_SLOT).map(([group, slot]) => [
      `.cm-pom-${group}, .cm-pom-${group} *`,
      { color: `${sectionColour(group, slot)} !important` },
    ]),
  ),
);

/** The extension a `pom.xml` descriptor installs as its `extraHighlight`. */
export const pomSections: Extension = [pomSectionPlugin, pomSectionTheme];
