/**
 * nemus language intelligence for CodeMirror — autocomplete + hover docs.
 *
 * Both features are driven by the **canonical DSL catalogue** (`referenceStore`,
 * fed by `nemus_lang_reference`) plus the file's **local symbols** (let / fn /
 * track / import) read straight from the live Tree-sitter tree (`nemus-lang`).
 * Keeping the data in the catalogue means the editor's hints can never drift from
 * the evaluator, and adding a builtin updates the editor for free.
 *
 *   - {@link nemusCompletion} — a `CompletionSource`. After `.` it offers the
 *     methods (transforms + signal methods); at the start of an identifier it
 *     offers combinators, generators, signals, log functions, islands, keywords,
 *     and the user's own declarations. Triggered as-you-type and on Ctrl+Space.
 *   - {@link nemusHover} — a `hoverTooltip` that renders the signature, summary,
 *     parameter table, and example for the identifier under the cursor (or the
 *     kind/label of a local symbol).
 */

import {
  autocompletion, completionKeymap,
  type Completion, type CompletionContext, type CompletionResult, type CompletionSource,
} from '@codemirror/autocomplete';
import { hoverTooltip, keymap, type Tooltip } from '@codemirror/view';
import type { EditorView } from '@codemirror/view';
import type { Extension } from '@codemirror/state';

import type { NemusDslEntry, NemusDslKind, NemusInstrument } from '$lib/ipc/nemus';
import { getNemusTree } from './nemus-cm';
import {
  extractSymbols, identifierAt, stringArgCallAt,
  type NemusSymbol, type NemusSymbolKind,
} from './nemus-lang';

/** A read of the live catalogue (the store, snapshotted at call time). */
export interface NemusIntelSource {
  /** All catalogue entries (already loaded; may be empty before the load lands). */
  entries(): NemusDslEntry[];
  /** Resolve a name to its (first) entry, or undefined. */
  byName(name: string): NemusDslEntry | undefined;
  /** The resolvable instruments (registry introspection) — offered as value
   *  completions inside `inst("…")`. May be empty before the registry loads. */
  instruments(): NemusInstrument[];
}

// ── Completion ─────────────────────────────────────────────────────────────────

/** Kinds offered after a `.` (method position) — transforms + signal methods +
 *  range/list combinators (`.par`/`.seq`/`.cat`/`.map`). */
const METHOD_KINDS = new Set<NemusDslKind>(['transform', 'signal_method', 'seq_method']);

/** Kinds offered at the start of an identifier (call / value position). */
const TOPLEVEL_KINDS = new Set<NemusDslKind>([
  'combinator', 'generator', 'signal', 'island', 'keyword', 'log',
]);

/** Map a catalogue kind to a CodeMirror completion `type` (drives the icon). */
function completionType(kind: NemusDslKind): string {
  switch (kind) {
    case 'keyword':       return 'keyword';
    case 'signal':        return 'variable';
    case 'note':          return 'constant';
    case 'mini':          return 'text';
    default:              return 'function';
  }
}

/** Map a local-symbol kind to a completion `type`. */
function symbolType(kind: NemusSymbolKind): string {
  return kind === 'fn' ? 'function' : 'variable';
}

/** Build a completion item from a catalogue entry. `detail` is the signature
 *  (shown inline); `info` is the summary + example (shown in the side panel). */
function entryCompletion(e: NemusDslEntry): Completion {
  return {
    label: e.name,
    type: completionType(e.kind),
    detail: e.signature,
    info: () => infoDom(e),
    boost: e.kind === 'transform' || e.kind === 'combinator' ? 1 : 0,
  };
}

/** Articulations the renderer always honours regardless of the instrument.
 *  `legato` is the monophonic re-glide machinery in the voice pool (it smooths
 *  phrasing even on a plain sustain patch with no dedicated legato samples), so
 *  it is offered for every voice — see arbor-nemus-audio's voice pool docs. */
const UNIVERSAL_ARTICULATIONS = ['legato'];

/** Build the completion list for `.art("…")` — the union of every instrument's
 *  declared articulations plus the universal ones, deduped + sorted. (We offer
 *  the full set rather than resolving the track's specific instrument: the call
 *  chain may not name one, and an over-broad list still beats no completion.) */
function articulationCompletions(instruments: NemusInstrument[]): Completion[] {
  const seen = new Set<string>();
  const out: Completion[] = [];
  const add = (name: string, universal: boolean) => {
    if (seen.has(name)) return;
    seen.add(name);
    out.push({ label: name, type: 'enum', detail: universal ? 'articulation' : undefined });
  };
  for (const a of UNIVERSAL_ARTICULATIONS) add(a, true);
  for (const i of instruments) for (const a of i.articulations) add(a, false);
  out.sort((a, b) => a.label.localeCompare(b.label));
  return out;
}

/** Build a completion item for a resolvable instrument (offered inside `inst`).
 *  Synths sort first (always available); the side `info` lists articulations. */
function instrumentCompletion(i: NemusInstrument): Completion {
  return {
    label: i.name,
    type: i.kind === 'synth' ? 'variable' : 'class',
    detail: i.kind, // synth · sample · sfz
    info: i.articulations.length ? `Articulations: ${i.articulations.join(', ')}` : undefined,
    boost: i.kind === 'synth' ? 1 : 0,
  };
}

/** Build a completion item from a local declaration. */
function symbolCompletion(s: NemusSymbol): Completion {
  return {
    label: s.name,
    type: symbolType(s.kind),
    detail: s.kind === 'fn' ? s.label : s.kind, // `bassline(root)` or `let`/`track`/`import`
  };
}

/** Local declarations the editor can offer (let / fn / track / import) for the
 *  current document — parsed from the live tree if available. */
function localSymbols(view: EditorView | undefined): NemusSymbol[] {
  if (!view) return [];
  const tree = getNemusTree(view);
  if (!tree) return [];
  try {
    const { defs, imports } = extractSymbols(tree);
    const out = [...defs.values()];
    // Imports aren't in `defs` (they map name → path); surface them too.
    for (const name of imports.keys()) {
      if (!defs.has(name)) {
        out.push({ id: `import:${name}`, kind: 'import', label: name, name, line: 0, offset: 0 });
      }
    }
    return out;
  } catch {
    return [];
  }
}

/**
 * The nemus completion source. Detects method position (a `.` immediately before
 * the word) to switch between method completions and top-level completions, and
 * always mixes in the user's local declarations at top level.
 */
export function nemusCompletion(src: NemusIntelSource): CompletionSource {
  return (context: CompletionContext): CompletionResult | null => {
    // Inside a string argument: offer scoped value completions. `inst("…")` gets
    // the live instrument registry; any other string suppresses the language
    // completions (a transform name inside a quoted path is never wanted).
    const tree = context.view ? getNemusTree(context.view) : null;
    if (tree) {
      const call = stringArgCallAt(tree, context.pos);
      if (call) {
        if (call.fn === 'inst') {
          const opts = src.instruments().map(instrumentCompletion);
          if (opts.length === 0) return null;
          // Dotted names (`synth.lead`, `strings.violin`) stay valid as you type.
          return { from: call.from, options: opts, validFor: /^[A-Za-z0-9_.\/-]*$/ };
        }
        if (call.fn === 'art') {
          const opts = articulationCompletions(src.instruments());
          if (opts.length === 0) return null;
          return { from: call.from, options: opts, validFor: /^[A-Za-z0-9_.-]*$/ };
        }
        // Any other string argument: suppress the language completions (a builtin
        // name inside a quoted path/pattern is never what the user wants).
        return null;
      }
    }

    const word = context.matchBefore(/[A-Za-z_][A-Za-z0-9_]*/);
    // Only auto-open once there's a word; explicit (Ctrl+Space) always opens.
    if (!context.explicit && (!word || word.from === word.to)) return null;
    const from = word ? word.from : context.pos;

    // Method position: a `.` immediately before the word start (`pat.gai|`).
    const before = context.state.sliceDoc(Math.max(0, from - 1), from);
    const isMethod = before === '.';

    const entries = src.entries();
    const options: Completion[] = [];

    if (isMethod) {
      for (const e of entries) if (METHOD_KINDS.has(e.kind)) options.push(entryCompletion(e));
    } else {
      const seen = new Set<string>();
      for (const e of entries) {
        if (!TOPLEVEL_KINDS.has(e.kind)) continue;
        if (seen.has(e.name)) continue; // dedupe alias names across kinds
        seen.add(e.name);
        options.push(entryCompletion(e));
      }
      for (const s of localSymbols(context.view)) {
        if (seen.has(s.name)) continue;
        seen.add(s.name);
        options.push(symbolCompletion(s));
      }
    }

    if (options.length === 0) return null;
    return { from, options, validFor: /^[A-Za-z_][A-Za-z0-9_]*$/ };
  };
}

// ── Hover docs ─────────────────────────────────────────────────────────────────

/** The hover tooltip extension: signature + summary + params + example for the
 *  identifier under the cursor, resolved against the catalogue (or, failing that,
 *  the file's local declarations). */
export function nemusHover(src: NemusIntelSource): Extension {
  return hoverTooltip((view, pos): Tooltip | null => {
    const tree = getNemusTree(view);
    if (!tree) return null;
    const name = identifierAt(tree, pos);
    if (!name) return null;

    const entry = src.byName(name);
    const local = entry ? undefined : localSymbols(view).find((s) => s.name === name);
    if (!entry && !local) return null;

    // Span the whole identifier so the tooltip anchors nicely.
    const word = wordRangeAt(view, pos);

    return {
      pos: word.from,
      end: word.to,
      above: true,
      create() {
        const inner = entry ? infoDom(entry) : symbolDom(local!);
        const dom = document.createElement('div');
        dom.className = 'cm-grv-hover';
        dom.appendChild(inner);
        return { dom };
      },
    };
  });
}

/** The identifier range covering `pos` (a-z, 0-9, _, .), for tooltip anchoring. */
function wordRangeAt(view: EditorView, pos: number): { from: number; to: number } {
  const line = view.state.doc.lineAt(pos);
  const text = line.text;
  const rel = pos - line.from;
  const isWord = (c: string) => /[A-Za-z0-9_]/.test(c);
  let s = rel, e = rel;
  while (s > 0 && isWord(text[s - 1])) s--;
  while (e < text.length && isWord(text[e])) e++;
  return { from: line.from + s, to: line.from + e };
}

// ── Shared DOM rendering (autocomplete `info` + hover) ─────────────────────────

/** Render a catalogue entry to a docs DOM block (signature, summary, params,
 *  example). Styled by `.cm-grv-doc*` classes (see nemus-cm theme). */
function infoDom(e: NemusDslEntry): HTMLElement {
  const root = document.createElement('div');
  root.className = 'cm-grv-doc';

  const sig = document.createElement('div');
  sig.className = 'cm-grv-doc-sig';
  sig.textContent = e.signature;
  root.appendChild(sig);

  if (e.summary) {
    const sum = document.createElement('div');
    sum.className = 'cm-grv-doc-summary';
    sum.textContent = e.summary;
    root.appendChild(sum);
  }

  if (e.params.length > 0) {
    const list = document.createElement('dl');
    list.className = 'cm-grv-doc-params';
    for (const p of e.params) {
      const dt = document.createElement('dt');
      dt.textContent = p.optional ? `${p.name}?` : p.name;
      const dd = document.createElement('dd');
      dd.textContent = p.default ? `${p.summary} (default ${p.default})` : p.summary;
      list.appendChild(dt);
      list.appendChild(dd);
    }
    root.appendChild(list);
  }

  if (e.example) {
    const ex = document.createElement('pre');
    ex.className = 'cm-grv-doc-example';
    ex.textContent = e.example;
    root.appendChild(ex);
  }

  return root;
}

/** Render a local declaration (no catalogue docs) to a minimal DOM block. */
function symbolDom(s: NemusSymbol): HTMLElement {
  const root = document.createElement('div');
  root.className = 'cm-grv-doc';
  const sig = document.createElement('div');
  sig.className = 'cm-grv-doc-sig';
  sig.textContent = s.kind === 'fn' ? s.label : `${s.kind} ${s.name}`;
  root.appendChild(sig);
  const sum = document.createElement('div');
  sum.className = 'cm-grv-doc-summary';
  sum.textContent =
    s.kind === 'fn' ? 'A function defined in this file.'
    : s.kind === 'let' ? 'A value bound in this file.'
    : s.kind === 'track' ? 'A track (mixer channel) in this file.'
    : 'An imported declaration.';
  root.appendChild(sum);
  return root;
}

// ── Bundled extension ──────────────────────────────────────────────────────────

/** Autocomplete + hover, wired to one catalogue source. Drop into the editor's
 *  extension list. Bundles the completion keymap (Ctrl+Space to open, ↑/↓ to
 *  move, Enter to accept, Esc to dismiss) so the feature is self-contained. */
export function nemusLanguageIntel(src: NemusIntelSource): Extension {
  return [
    autocompletion({
      override: [nemusCompletion(src)],
      activateOnTyping: true,
      icons: true,
    }),
    keymap.of(completionKeymap),
    nemusHover(src),
  ];
}
