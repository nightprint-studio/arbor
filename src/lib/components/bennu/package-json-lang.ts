/**
 * The editor language for a **`package.json`**.
 *
 * The base is the ordinary JSON grammar. What is added is what a generic JSON mode cannot know,
 * because it is not in the file's *syntax* but in what npm does with it:
 *
 * - **the sections are named things.** `scripts`, `dependencies`, `engines` are not keys like any
 *   other; they are the headings a reader scrolls to. Coloured as headings, so a five-hundred-line
 *   manifest has landmarks instead of four hundred identical blue strings.
 * - **a version range says how pinned it is.** `^5.0.0` floats, `5.0.0` does not, and
 *   `workspace:*` is not a version at all. Three colours, and the answer to "what will `npm
 *   install` actually change" becomes something you can see rather than something you read.
 *
 * The version *hints* — "there is a newer release of this" — and the run controls over `scripts`
 * are not here: they are code lenses, drawn by the editor host from what the backend answers, the
 * same way a `Cargo.toml`'s are. See `BennuEditor.svelte`.
 *
 * ## Why decorate from the syntax tree
 *
 * A regex over the text would have to track which section it is inside, and it would be wrong
 * about the one thing that matters: `"scripts"` nested inside `exports` is not the scripts
 * section. The Lezer tree already knows the depth and the parent, exactly, and re-parses
 * incrementally as you type.
 */

import { json } from '@codemirror/lang-json';
import { syntaxTree } from '@codemirror/language';
import { RangeSetBuilder, type Extension } from '@codemirror/state';
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from '@codemirror/view';
import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';

/** Whether a path is an npm manifest.
 *
 *  The **name**, not the extension: a project is full of `.json` files that are not manifests, and
 *  a `tsconfig.json` given these rules would have every key in it coloured as a dependency.
 *  `node_modules` is out for a different reason — every installed package has one, and none of them
 *  is yours to edit. Mirrors `bennu_npm::is_package_manifest`, which is the gate on the other side. */
export function isPackageManifest(path: string | null | undefined): boolean {
  if (!path) return false;
  const norm = path.replace(/\\/g, '/');
  if (norm.split('/').includes('node_modules')) return false;
  return (norm.split('/').pop() ?? '').toLowerCase() === 'package.json';
}

/** The sections worth marking as headings, and nothing else — a list that grew to cover every key
 *  npm has ever defined would be a list where nothing stands out. */
const DEPENDENCY_SECTIONS = new Set([
  'dependencies', 'devDependencies', 'peerDependencies', 'optionalDependencies',
]);
const SECTIONS = new Set([
  ...DEPENDENCY_SECTIONS,
  'scripts', 'engines', 'workspaces', 'exports', 'imports', 'overrides', 'resolutions',
  'peerDependenciesMeta', 'packageManager', 'bin', 'files',
]);

const SECTION_MARK = Decoration.mark({ class: 'cm-pj-section' });
const SCRIPT_MARK = Decoration.mark({ class: 'cm-pj-script' });
const FLOATS_MARK = Decoration.mark({ class: 'cm-pj-floats' });
const PINNED_MARK = Decoration.mark({ class: 'cm-pj-pinned' });
const ELSEWHERE_MARK = Decoration.mark({ class: 'cm-pj-elsewhere' });

/**
 * How much a declared range can move under you.
 *
 * Three answers and not five: this is a glance, not a semver lecture. `^` and `~` and every
 * comparator float; a bare version does not; anything with a protocol in it does not come from the
 * registry at all and is a different kind of thing entirely — which is precisely what somebody
 * scanning a manifest for "why did that change" needs to see first.
 */
function rangeMark(text: string): Decoration {
  const t = text.trim();
  if (!t) return ELSEWHERE_MARK;
  if (/^[\^~>=<*]/.test(t) || t.includes('||') || /\bx\b/.test(t) || t === '*') return FLOATS_MARK;
  if (/^v?\d/.test(t)) return PINNED_MARK;
  // `workspace:`, `file:`, `link:`, `npm:alias@…`, `git+…`, `github:owner/repo`.
  return ELSEWHERE_MARK;
}

/** The decorations for one state, walked off the JSON tree. */
function decorate(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const tree = syntaxTree(view.state);
  const doc = view.state.doc;

  // The document's one top-level object. `JsonText → Object`; anything else is a manifest being
  // typed, and there is nothing to say about it yet.
  const root = tree.topNode.firstChild;
  if (!root || root.name !== 'Object') return builder.finish();

  for (let prop = root.firstChild; prop; prop = prop.nextSibling) {
    if (prop.name !== 'Property') continue;
    const nameNode = prop.firstChild;
    if (!nameNode || nameNode.name !== 'PropertyName') continue;
    const key = doc.sliceString(nameNode.from + 1, nameNode.to - 1);
    if (!SECTIONS.has(key)) continue;
    builder.add(nameNode.from, nameNode.to, SECTION_MARK);

    // The section's own members. Only a string value is decorated: a nested object under
    // `exports` is structure, not a version or a command.
    const value = nameNode.nextSibling;
    if (!value || value.name !== 'Object') continue;
    const isScripts = key === 'scripts';
    const isDeps = DEPENDENCY_SECTIONS.has(key);
    if (!isScripts && !isDeps) continue;

    for (let member = value.firstChild; member; member = member.nextSibling) {
      if (member.name !== 'Property') continue;
      const memberValue = member.firstChild?.nextSibling;
      if (!memberValue || memberValue.name !== 'String') continue;
      // Inside the quotes: they are punctuation and belong to the JSON colouring.
      const from = memberValue.from + 1;
      const to = memberValue.to - 1;
      if (to <= from) continue;
      builder.add(from, to, isScripts ? SCRIPT_MARK : rangeMark(doc.sliceString(from, to)));
    }
  }
  return builder.finish();
}

const overlay = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorate(view);
    }
    update(u: ViewUpdate) {
      // On a re-parse as well as on an edit: the tree arrives asynchronously for a large document,
      // and decorations computed before it lands would be a manifest that colours itself a moment
      // after it opens and never again.
      if (u.docChanged || u.viewportChanged || syntaxTree(u.startState) !== syntaxTree(u.state)) {
        this.decorations = decorate(u.view);
      }
    }
  },
  { decorations: (v) => v.decorations },
);

const theme = EditorView.theme({
  // A heading. Weight and the type colour, not a background: a manifest has a dozen of these and a
  // dozen tinted blocks is a striped page.
  '.cm-pj-section': { color: 'var(--syntax-type, #4d9be6)', fontWeight: '700' },
  // A script body is a shell command. The keyword colour, because that is what it is — the one
  // string in the file that is *code*.
  '.cm-pj-script': { color: 'var(--syntax-keyword, #cc7832)' },
  // Floats on install. The warning hue, used here as "this can move", which is what warning means
  // everywhere else in the app.
  '.cm-pj-floats': { color: 'var(--warning, #d19a66)' },
  // Pinned. Green, and the only reason it earns a colour at all is that the contrast with the one
  // above is the whole information.
  '.cm-pj-pinned': { color: 'var(--success, #6a9955)' },
  // Not from the registry — a workspace sibling, a folder, a git URL. Muted: it is not a version,
  // and colouring it like one would be the wrong claim.
  '.cm-pj-elsewhere': { color: 'var(--text-muted)', fontStyle: 'italic' },
});

const extension: Extension = [json(), overlay, theme];

/**
 * The `package.json` descriptor.
 *
 * **No completion or hover hooks**, deliberately: nothing in the server catalogue serves `.json`,
 * so wiring them would be an IPC round-trip per keystroke to an engine with nothing to say about
 * the file. What a manifest gets instead of a language server is code lenses — the run controls
 * and the version offers — and those are the editor host's, drawn from what the backend parsed.
 *
 * Folding is the JSON grammar's own: immediate, with no process behind it.
 */
export const packageJsonLanguage: LanguageDescriptor = {
  id: 'package-json',
  createParser: () =>
    Promise.reject(new Error('package-json highlights from the JSON grammar')),
  classify: () => null,
  cmExtension: extension,
  cmFold: true,
};
