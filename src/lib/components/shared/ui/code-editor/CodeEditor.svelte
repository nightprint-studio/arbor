<script lang="ts">
  /**
   * CodeEditor — the generic, app-agnostic CodeMirror 6 host for one buffer.
   *
   * Generalised from merula's `MerulaEditor`, but with NO product/engine imports:
   * it is parametrised by a {@link LanguageDescriptor} (syntax highlight, go-to-decl)
   * and driven entirely through props. Controlled `value`: external writes (tab
   * switch, cross-file open) flow in via the prop; internal edits flow out via
   * `oninput`. Imperative API (focus / getValue / scrollToLineCol / scrollToOffset /
   * openSearch / setDiagnostics) is exposed via `bind:this`.
   *
   * Diagnostics arrive as {@link EditorDiagnostic}[] in **UTF-8 byte offsets**; they
   * are mapped onto CodeMirror's UTF-16 lint spans against the live buffer.
   */
  import { onDestroy } from 'svelte';
  import { EditorState, Compartment, StateEffect, StateField, type Extension } from '@codemirror/state';
  import {
    Decoration,
    EditorView,
    GutterMarker,
    gutter,
    placeholder as cmPlaceholder,
    type DecorationSet,
    type KeyBinding,
    type ViewUpdate,
  } from '@codemirror/view';
  import { RangeSet } from '@codemirror/state';
  import { indentUnit as cmIndentUnit } from '@codemirror/language';
  import { setDiagnostics as cmSetDiagnostics, type Diagnostic as CmDiagnostic } from '@codemirror/lint';
  import { openSearchPanel } from '@codemirror/search';

  import type { LanguageDescriptor, EditorDiagnostic, EditorViewSnapshot } from './types';
  import { createCodeEditorExtensions, refTextAt } from './extensions';
  import { minimapExtension } from './minimap';
  import { makeByteToU16 } from './highlight';

  let {
    value,
    language,
    readOnly = false,
    diagnostics = [],
    rulerColumn,
    minimap = false,
    emmet = false,
    indentGuides = false,
    stickyScroll = false,
    scrollbarOverview = false,
    tabSize,
    indentUnit,
    initialState,
    placeholder,
    wrap = false,
    lineNumbers = true,
    keyBindings,
    marks = [],
    lineHighlights = [],
    gutterMarks = [],
    onGutterClick,
    flagMarks,
    canFlag,
    onFlagClick,
    onFlagContext,
    onFlagsMoved,
    oninput,
    oncaret,
    onViewState,
    onfocus,
    onGoto,
  }: {
    value: string;
    language: LanguageDescriptor;
    readOnly?: boolean;
    /** Diagnostics in UTF-8 byte offsets — mapped to CM lint spans against the buffer. */
    diagnostics?: EditorDiagnostic[];
    /** Draw a vertical margin guide at this 1-based column (IntelliJ-style). Omit for none. */
    rulerColumn?: number;
    /** Show the right-gutter minimap overview. Toggled live via a compartment. */
    minimap?: boolean;
    /** Enable Emmet abbreviation expansion on Tab (markup buffers). Static at mount. */
    emmet?: boolean;
    /** Draw indentation guides (active block brightened). Static at mount. */
    indentGuides?: boolean;
    /** Pin enclosing declaration lines to the top (sticky scroll). Static at mount. */
    stickyScroll?: boolean;
    /** Replace the native scrollbar with the IntelliJ overview strip (diagnostic marks + hover
     *  preview). A host uses this INSTEAD of `minimap`. Static at mount. */
    scrollbarOverview?: boolean;
    /** Tab width in columns. Omit to keep CodeMirror's default (an editor that never sets
     *  indentation is unchanged). Applied live via a compartment. */
    tabSize?: number;
    /** The whitespace inserted for one indent level — `'\t'` for tabs, `'    '` for N
     *  spaces. Omit to keep CodeMirror's default. Applied live via a compartment. */
    indentUnit?: string;
    /** Cursor + scroll to restore at mount (e.g. the tab's last-known position). */
    initialState?: EditorViewSnapshot;
    /**
     * Grey text shown while the buffer is empty.
     *
     * For the editors that are a *field* rather than a document — a pattern box, a
     * snippet — where an example is the shortest explanation of the syntax there
     * is. Static at mount, like the rest of the extension set.
     */
    placeholder?: string;
    /**
     * Wrap long lines instead of scrolling sideways.
     *
     * Off for a document — a script has a column budget and horizontal scrolling
     * is how you notice you blew it. On for the editors that are a *field* in a
     * narrow box: a structural pattern is one long statement, and in a 300px
     * panel it scrolled out of its own left edge, so you could not see the start
     * of the thing you were typing. Static at mount, like the rest of the set.
     */
    wrap?: boolean;
    /**
     * Show the line-number gutter. `true` by default, because a buffer is navigated by line.
     *
     * Turn it off for a **short input** that wants an editor for the highlighting and the
     * completion rather than for the chrome — a structural query is two or three lines, and a
     * gutter numbering them is a column of noise beside a field. Static at mount, like the rest
     * of the set.
     */
    lineNumbers?: boolean;
    /**
     * Keys this host claims back from CodeMirror, e.g. `Mod-Enter` to run a
     * statement. Installed above every built-in binding — see the option of the
     * same name in `extensions.ts` for why that is not optional. Static at mount:
     * bind stable functions that read live state rather than swapping the array.
     */
    keyBindings?: readonly KeyBinding[];
    /**
     * Ranges to give a class to — a highlight the *host* decides, on top of
     * whatever the language highlights.
     *
     * For text that is not the buffer's language and should not be read as though
     * it were: Picus's SQL abbreviations are the case this exists for, since
     * `s#ordini(id)[stato='EV']` is a shorthand the backend expands and colouring
     * it as SQL makes correct input look like a mistake.
     *
     * Offsets are **UTF-16**, like everything else a host computes from the string
     * it passed in; the byte-offset API is the `diagnostics` prop, which comes from
     * a backend. Applied live, so a host can recompute them freely.
     */
    marks?: readonly { from: number; to: number; className: string }[];
    /**
     * Whole-line highlights — a class applied to the ROW, so the band spans the full width
     * however short the line is. For "execution is here", "this is the hunk", and anything
     * else whose subject is the line rather than the text on it.
     *
     * Distinct from {@link marks} on purpose: a mark paints the glyphs it covers and stops at
     * the last character, which reads as a patch rather than as a band.
     */
    lineHighlights?: readonly { line: number; className: string }[];
    /**
     * Icons for the left gutter, one per line — the affordance that makes a relationship
     * visible without being asked for it.
     *
     * The host owns what a mark means and what clicking it does; this only draws a glyph
     * with a tooltip and reports the click. `glyph` is rendered as text, so an emoji, an
     * arrow or a single letter all work and no icon set has to be agreed on across
     * products. Applied live, and empty by default — an editor that passes nothing gets
     * no gutter at all, not an empty column.
     */
    gutterMarks?: readonly { line: number; glyph: string; tooltip: string; className?: string }[];
    /** A gutter icon was clicked: its 1-based line, plus the event — so a host that has more
     *  than one thing to offer can anchor a menu where the pointer is instead of guessing. */
    onGutterClick?: (line: number, event: MouseEvent) => void;
    /**
     * A second gutter, for a per-line **toggle** the host owns: breakpoints, bookmarks.
     *
     * Separate from {@link gutterMarks} because the two answer different clicks. A mark opens
     * what it points at, and only lines that have one are clickable; a flag is *set and unset*,
     * so every line is a target and an unset line has to show that it is one. Sharing a column
     * would make one click mean two things on the lines that have both.
     *
     * Present but empty is meaningful: pass `[]` for a gutter with nothing in it yet (the column
     * still reserves its width and offers the hover affordance), and leave it undefined for no
     * second gutter at all.
     */
    flagMarks?: readonly { line: number; className?: string; tooltip?: string }[];
    /**
     * Which lines may carry a flag. Omit for "any of them".
     *
     * A host that knows some lines cannot hold one — a breakpoint needs a line that compiles to
     * bytecode — says so here, and those lines get no affordance and ignore the click. The
     * absence of the dot is the explanation: there is nothing to press, rather than a press
     * that quietly does something else.
     *
     * A line that already carries a flag is always offered, whatever this answers, or an edit
     * that invalidated the line underneath would leave a flag nobody can remove.
     */
    canFlag?: (line: number) => boolean;
    /** A line of the flag gutter was clicked — toggle it. */
    onFlagClick?: (line: number, event: MouseEvent) => void;
    /** Right-click on the flag gutter, for a host that offers more than on/off. */
    onFlagContext?: (line: number, event: MouseEvent) => void;
    /**
     * Editing moved some flagged lines.
     *
     * A flag is remembered by line number, and a line number is only true until someone types
     * above it: insert a line at the top of a file and every breakpoint in it is off by one.
     * CodeMirror knows where each position went, so the mapping is done here and reported once
     * per change — the host updates its own model and the new numbers come back through
     * {@link flagMarks}.
     */
    onFlagsMoved?: (moves: readonly { from: number; to: number }[]) => void;
    oninput?: (text: string) => void;
    /** Live caret position (1-based line/col) — drives a host footer Ln/Col. */
    oncaret?: (line: number, col: number) => void;
    /** Cursor + scroll changed — the host can persist it for a later {@link initialState}. */
    onViewState?: (s: EditorViewSnapshot) => void;
    onfocus?: () => void;
    /** Ctrl/Cmd+Click on an identifier the descriptor didn't resolve locally — the word
     *  plus the clicked position as a UTF-8 byte offset (for a BE go-to-declaration). */
    onGoto?: (word: string, view: EditorView, byteOffset: number) => void;
  } = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  // Indentation lives in its own compartment so a footer change (tab size / tabs-vs-spaces)
  // reconfigures the OPEN buffer live, without a remount.
  const indentCompartment = new Compartment();
  // Minimap in its own compartment so the setting toggle reconfigures the OPEN buffer live.
  const minimapCompartment = new Compartment();
  let suppressEmit = false;

  /** The `EditorState.tabSize` + `indentUnit` facets for the current props — empty when the
   *  host sets neither (so non-indent-aware editors keep CodeMirror's defaults untouched). */
  function indentExtensions(): Extension[] {
    const e: Extension[] = [];
    if (tabSize !== undefined) e.push(EditorState.tabSize.of(tabSize));
    if (indentUnit !== undefined) e.push(cmIndentUnit.of(indentUnit));
    return e;
  }
  let lastEmitted: string | null = null;
  // Scroll-listener teardown (emits `onViewState` so the host can persist scroll too).
  let detachScroll: (() => void) | null = null;
  let scrollRaf = 0;

  /** Report the current cursor + scroll to the host (for per-tab restore). */
  function emitViewState() {
    if (!view || !onViewState) return;
    const sel = view.state.selection.main;
    onViewState({ anchor: sel.anchor, head: sel.head, scrollTop: view.scrollDOM.scrollTop });
  }

  // ── Byte-span diagnostics → CM lint markers ───────────────────────────────────

  /**
   * A diagnostic message is meant to be a sentence, and every backend here writes
   * one — but nothing in the contract enforces it, and the way it breaks is that a
   * message quotes the text it is complaining about. Rendered whole that is a wall
   * over the editor that reads as a crash, which is a bad way to learn a line is
   * wrong. Clamped, a long message stays readable and a pathological one stays a
   * message. The tooltip also scrolls (see `theme.ts`); this keeps the DOM node
   * itself from being the size of the file.
   */
  const MAX_DIAGNOSTIC_CHARS = 2000;
  function clampMessage(message: string): string {
    return message.length <= MAX_DIAGNOSTIC_CHARS
      ? message
      : message.slice(0, MAX_DIAGNOSTIC_CHARS) + ' […]';
  }

  function toCmDiagnostics(errors: EditorDiagnostic[], src: string): CmDiagnostic[] {
    const b2u = makeByteToU16(src);
    const len = src.length;
    const out: CmDiagnostic[] = [];
    for (const e of errors) {
      let from = b2u(e.from);
      let to = b2u(e.to);
      from = Math.max(0, Math.min(from, len));
      to = Math.max(from, Math.min(to, len));
      if (to === from) to = Math.min(len, from + 1); // give the marker some width
      out.push({ from, to, severity: e.severity, message: clampMessage(e.message), actions: e.actions });
    }
    return out;
  }

  function pushDiagnostics() {
    if (!view) return;
    const src = view.state.doc.toString();
    view.dispatch(cmSetDiagnostics(view.state, toCmDiagnostics(diagnostics, src)));
  }
  // Re-push whenever the diagnostics prop changes.
  $effect(() => { void diagnostics; pushDiagnostics(); });

  // ── Host-supplied marks ───────────────────────────────────────────────────────
  //
  // A `StateField` rather than a `ViewPlugin`, because the ranges come from outside
  // CodeMirror: the host recomputes them from its own state, and the field's job is
  // only to hold the last set handed in. `map`ping them through document changes is
  // what keeps a highlight in place for the frame between a keystroke and the
  // host's recomputation, instead of flickering off and back on.
  const setMarks = StateEffect.define<DecorationSet>();
  const markField = StateField.define<DecorationSet>({
    create: () => Decoration.none,
    update(current, tr) {
      for (const effect of tr.effects) if (effect.is(setMarks)) return effect.value;
      return current.map(tr.changes);
    },
    provide: (field) => EditorView.decorations.from(field),
  });

  function pushMarks() {
    if (!view) return;
    const len = view.state.doc.length;
    const ranges = marks
      .map((m) => ({
        from: Math.max(0, Math.min(m.from, len)),
        to: Math.max(0, Math.min(m.to, len)),
        className: m.className,
      }))
      .filter((m) => m.to > m.from)
      .sort((a, b) => a.from - b.from)
      .map((m) => Decoration.mark({ class: m.className }).range(m.from, m.to));
    view.dispatch({ effects: setMarks.of(Decoration.set(ranges, true)) });
  }
  $effect(() => { void marks; pushMarks(); });

  // ── Whole-line highlights ─────────────────────────────────────────────────────
  //
  // A LINE decoration, not a mark: a mark paints the glyphs it covers, so a "current line"
  // drawn that way ends at the last character and leaves the rest of the row bare — which
  // reads as a patch of colour rather than as a band, and is exactly wrong for the one thing
  // it is meant to say (*this* row). A line decoration puts the class on the row itself.
  const setLineMarks = StateEffect.define<DecorationSet>();
  const lineMarkField = StateField.define<DecorationSet>({
    create: () => Decoration.none,
    update(current, tr) {
      for (const effect of tr.effects) if (effect.is(setLineMarks)) return effect.value;
      return current.map(tr.changes);
    },
    provide: (field) => EditorView.decorations.from(field),
  });

  function pushLineHighlights() {
    if (!view) return;
    const lines = view.state.doc.lines;
    const ranges = lineHighlights
      .filter((h) => h.line >= 1 && h.line <= lines)
      .sort((a, b) => a.line - b.line)
      .map((h) => Decoration.line({ class: h.className }).range(view!.state.doc.line(h.line).from));
    view.dispatch({ effects: setLineMarks.of(Decoration.set(ranges, true)) });
  }
  $effect(() => { void lineHighlights; pushLineHighlights(); });

  // ── Host-supplied gutter icons ────────────────────────────────────────────────
  //
  // Same shape as the marks above and for the same reason: the host recomputes them from
  // its own state and this field holds the last set. The gutter extension is installed
  // only when the host actually passes marks — an editor that doesn't use them gets no
  // extra column, which matters because an empty gutter still costs horizontal space in
  // every editor in the app.
  // Plain fields, not constructor parameter properties: Svelte's compiler parses this
  // script without a TypeScript transform, so `constructor(private x: string)` is a
  // syntax error here however valid it is in a `.ts` file.
  class HostGutterMarker extends GutterMarker {
    glyph: string;
    tooltip: string;
    markerClass: string;

    constructor(glyph: string, tooltip: string, markerClass: string) {
      super();
      this.glyph = glyph;
      this.tooltip = tooltip;
      this.markerClass = markerClass;
    }

    // No `override` modifier for the same reason as above — another TypeScript-only
    // keyword this script is not preprocessed for.
    toDOM() {
      const el = document.createElement('span');
      el.className = `cm-host-gutter-icon ${this.markerClass}`.trim();
      el.textContent = this.glyph;
      el.title = this.tooltip;
      return el;
    }
  }

  const setGutter = StateEffect.define<RangeSet<GutterMarker>>();
  const gutterField = StateField.define<RangeSet<GutterMarker>>({
    create: () => RangeSet.empty,
    update(current, tr) {
      for (const effect of tr.effects) if (effect.is(setGutter)) return effect.value;
      return current.map(tr.changes);
    },
  });

  function pushGutter() {
    if (!view) return;
    const lines = view.state.doc.lines;
    const ranges = gutterMarks
      .filter((m) => m.line >= 1 && m.line <= lines)
      // One icon per line: two marks on the same line would be two ranges at the same
      // position, and the gutter draws only the first anyway.
      .filter((m, i, all) => all.findIndex((o) => o.line === m.line) === i)
      .sort((a, b) => a.line - b.line)
      .map((m) =>
        new HostGutterMarker(m.glyph, m.tooltip, m.className ?? '').range(
          view!.state.doc.line(m.line).from,
        ),
      );
    view.dispatch({ effects: setGutter.of(RangeSet.of(ranges, true)) });
  }
  $effect(() => { void gutterMarks; pushGutter(); });

  /**
   * The gutter extension. Always installed, never conditional on the current marks: they
   * arrive asynchronously (a host fetches them from a backend), so deciding at mount
   * whether to have a gutter would mean never having one. With no marks the field is
   * empty and the column collapses to nothing — no spacer, so an editor that never uses
   * this pays no horizontal space.
   */
  const hostGutter = [
    gutterField,
    gutter({
      class: 'cm-host-gutter',
      markers: (v) => v.state.field(gutterField, false) ?? RangeSet.empty,
      domEventHandlers: {
        mousedown(v, line, event) {
          onGutterClick?.(v.state.doc.lineAt(line.from).number, event as MouseEvent);
          return true;
        },
      },
    }),
  ];

  // ── The flag gutter (a per-line toggle: breakpoints, bookmarks) ───────────────
  //
  // Rendered with `lineMarker` rather than from a RangeSet, because unlike the icon gutter
  // EVERY line is a target here: a line with no flag still has to say it can have one, which
  // it does by showing a faint dot under the pointer. So each rendered line gets a marker —
  // set or empty — and the CSS decides which of the two you can see.
  class FlagMarker extends GutterMarker {
    on: boolean;
    tooltip: string;
    markerClass: string;

    constructor(on: boolean, tooltip: string, markerClass: string) {
      super();
      this.on = on;
      this.tooltip = tooltip;
      this.markerClass = markerClass;
    }

    eq(other: FlagMarker) {
      return (
        this.on === other.on
        && this.tooltip === other.tooltip
        && this.markerClass === other.markerClass
      );
    }

    toDOM() {
      const el = document.createElement('span');
      el.className = `cm-flag-icon ${this.on ? 'cm-flag-on' : ''} ${this.markerClass}`.trim();
      if (this.tooltip) el.title = this.tooltip;
      return el;
    }
  }

  /** Bump this and the gutter re-renders its line markers — the effect a host's new
   *  {@link flagMarks} rides in on, since `lineMarker` reads them out of the closure. */
  const flagsChanged = StateEffect.define<null>();

  function flagAt(line: number) {
    return flagMarks?.find((f) => f.line === line);
  }

  /** Whether this line is a target at all. A line that already carries one always is — see
   *  {@link canFlag} — so an edit can never strand a flag on a line that has stopped
   *  qualifying. */
  function flaggable(line: number): boolean {
    return !!flagAt(line) || !canFlag || canFlag(line);
  }

  /**
   * The gutter extension. **Always installed**, never conditional on the props as they stand at
   * mount — the same rule the icon gutter above states, and for a sharper reason here: a host's
   * flags are typically hydrated from a backend *after* the editor exists, so deciding at mount
   * whether to have this column would mean never having it.
   *
   * What decides whether the column is *visible* is `flagMarks` being defined at all. Undefined
   * → no marker on any line → the column has no content and collapses to nothing, so an editor
   * that does not use flags pays no horizontal space. Defined (even empty) → every line gets a
   * marker, visible or not, which is what gives a line with no flag something to hover.
   */
  const flagGutter = gutter({
    class: 'cm-flag-gutter',
    lineMarker: (v, block) => {
      if (!flagMarks) return null;
      const line = v.state.doc.lineAt(block.from).number;
      // No marker at all on a line that cannot take one — not an invisible one. The gutter
      // then has nothing to hover there, which is the whole of how the rule is communicated.
      if (!flaggable(line)) return null;
      const found = flagAt(line);
      return new FlagMarker(!!found, found?.tooltip ?? '', found?.className ?? '');
    },
    lineMarkerChange: (u) =>
      u.docChanged || u.transactions.some((tr) => tr.effects.some((e) => e.is(flagsChanged))),
    domEventHandlers: {
      mousedown(v, block, event) {
        const e = event as MouseEvent;
        // Left button only: a right-click is the context menu below, and a middle-click
        // pasting a breakpoint in is nobody's intent.
        if (e.button !== 0 || !onFlagClick) return false;
        const line = v.state.doc.lineAt(block.from).number;
        if (!flaggable(line)) return true; // swallowed: the line showed no affordance
        onFlagClick(line, e);
        return true;
      },
      contextmenu(v, block, event) {
        if (!onFlagContext) return false;
        const e = event as MouseEvent;
        const line = v.state.doc.lineAt(block.from).number;
        if (!flaggable(line)) return true;
        e.preventDefault();
        onFlagContext(line, e);
        return true;
      },
    },
  });

  // Both are read out of the closure by `lineMarker`, so a new set of either has to be
  // announced to the gutter — a host recomputing which lines qualify (a re-parse of the buffer)
  // changes what the column offers just as much as a new flag does.
  $effect(() => {
    void flagMarks;
    void canFlag;
    view?.dispatch({ effects: flagsChanged.of(null) });
  });

  /** Where each flagged line ended up after an edit. Empty when nothing moved, which is the
   *  overwhelming majority of keystrokes — typing inside a line moves no line at all. */
  function movedFlags(u: ViewUpdate): { from: number; to: number }[] {
    const moves: { from: number; to: number }[] = [];
    for (const flag of flagMarks ?? []) {
      if (flag.line < 1 || flag.line > u.startState.doc.lines) continue;
      const at = u.startState.doc.line(flag.line).from;
      const to = u.state.doc.lineAt(u.changes.mapPos(at, 1)).number;
      if (to !== flag.line) moves.push({ from: flag.line, to });
    }
    return moves;
  }

  function mount(target: HTMLDivElement) {
    const { extensions } = createCodeEditorExtensions(language, {
      readOnly, onGoto, rulerColumn, emmet, indentGuides, stickyScroll, scrollbarOverview,
      keyBindings, lineNumbers,
    });

    const updateListener = EditorView.updateListener.of((u) => {
      if (u.docChanged && !suppressEmit) {
        const text = u.state.doc.toString();
        lastEmitted = text;
        oninput?.(text);
      }
      if (u.docChanged && onFlagsMoved) {
        const moves = movedFlags(u);
        if (moves.length) onFlagsMoved(moves);
      }
      if (u.focusChanged && u.view.hasFocus) onfocus?.();
      if (u.selectionSet || u.docChanged) {
        if (oncaret) {
          const head = u.state.selection.main.head;
          const line = u.state.doc.lineAt(head);
          oncaret(line.number, head - line.from + 1);
        }
        emitViewState();
      }
    });

    const state = EditorState.create({
      doc: value,
      extensions: [
        // First, so the toggle column sits OUTSIDE the line numbers — furthest from the text
        // and hardest to hit by accident, which is where every IDE puts breakpoints. Gutter
        // order follows extension precedence, and earlier is further left.
        flagGutter,
        extensions,
        markField,
        lineMarkField,
        hostGutter,
        indentCompartment.of(indentExtensions()),
        minimapCompartment.of(minimap ? minimapExtension() : []),
        placeholder ? cmPlaceholder(placeholder) : [],
        wrap ? EditorView.lineWrapping : [],
        updateListener,
      ],
    });
    view = new EditorView({ state, parent: target });
    pushDiagnostics();
    pushMarks();
    pushLineHighlights();
    pushGutter();

    // Restore the host-provided cursor + scroll (per-tab position). The scroll is set
    // after a frame so the layout the offset refers to exists.
    if (initialState) {
      const len = view.state.doc.length;
      const anchor = Math.min(Math.max(0, initialState.anchor), len);
      const head = Math.min(Math.max(0, initialState.head), len);
      view.dispatch({ selection: { anchor, head } });
      const top = initialState.scrollTop;
      requestAnimationFrame(() => { if (view) view.scrollDOM.scrollTop = top; });
    }

    // Persist scroll changes too (selection changes come through the update listener).
    const onScroll = () => {
      if (scrollRaf) return;
      scrollRaf = requestAnimationFrame(() => { scrollRaf = 0; emitViewState(); });
    };
    view.scrollDOM.addEventListener('scroll', onScroll, { passive: true });
    detachScroll = () => view?.scrollDOM.removeEventListener('scroll', onScroll);

    // WebView2/Windows: a freshly-created EditorView in a just-shown container (the editor
    // remounts via `{#key activePath}` on every tab switch / go-to navigation) can paint BLANK
    // until an event forces a re-measure — the reported "black tab until you click it". Force a
    // measure once layout exists. Double rAF: one frame for the container to lay out, one to
    // paint. `requestMeasure` is idempotent, so this is harmless if the view already painted.
    requestAnimationFrame(() => {
      view?.requestMeasure();
      requestAnimationFrame(() => view?.requestMeasure());
    });
  }

  $effect(() => { if (hostEl && !view) mount(hostEl); });

  // Live indentation reconfigure — a footer change to tab size / tabs-vs-spaces applies to
  // the already-open buffer without a remount. Reads the props so it re-runs on change.
  $effect(() => {
    const ts = tabSize, iu = indentUnit; // tracked deps
    void ts; void iu;
    view?.dispatch({ effects: indentCompartment.reconfigure(indentExtensions()) });
  });

  // Live minimap toggle — reconfigure the open buffer when the setting flips (no remount).
  $effect(() => {
    const on = minimap; // tracked dep
    view?.dispatch({ effects: minimapCompartment.reconfigure(on ? minimapExtension() : []) });
  });

  onDestroy(() => {
    if (scrollRaf) cancelAnimationFrame(scrollRaf);
    detachScroll?.();
    view?.destroy();
    view = undefined;
  });

  // ── value (controlled) → editor ───────────────────────────────────────────────
  $effect(() => {
    const next = value;
    if (!view) return;
    if (next === lastEmitted) return;
    const current = view.state.doc.toString();
    if (current === next) return;
    suppressEmit = true;
    try {
      view.dispatch({ changes: { from: 0, to: current.length, insert: next } });
    } finally { suppressEmit = false; }
  });

  // ── Imperative API ────────────────────────────────────────────────────────────
  export function focus() { view?.focus(); }

  export function getValue(): string {
    return view?.state.doc.toString() ?? value;
  }

  /** The caret head as a **UTF-8 byte offset** (what byte-offset backends want, e.g.
   *  a rename / find-usages query). CodeMirror positions are UTF-16 code units, so we
   *  measure the encoded length of the text before the head. 0 when unmounted. */
  export function caretByteOffset(): number {
    if (!view) return 0;
    const head = view.state.selection.main.head;
    return new TextEncoder().encode(view.state.doc.sliceString(0, head)).length;
  }

  /** Open CodeMirror's search panel + focus its query field (routed here from the
   *  host's Ctrl+F when the editor pane has focus). */
  export function openSearch() {
    if (view) openSearchPanel(view);
  }

  export function scrollToOffset(offset: number, select = false) {
    if (!view) return;
    const len = view.state.doc.length;
    const pos = Math.max(0, Math.min(offset, len));
    view.dispatch({
      selection: select ? { anchor: pos, head: pos } : { anchor: pos },
      effects: EditorView.scrollIntoView(pos, { y: 'center' }),
    });
    view.focus();
  }

  /** Move the caret to a **UTF-8 byte offset** and reveal it (centred). Backend spans
   *  (diagnostics, form/field ranges) are byte offsets, so we map through
   *  `makeByteToU16` against the live buffer before dispatching — the byte-aware sibling
   *  of {@link scrollToOffset}. No-op when unmounted. */
  export function scrollToByteOffset(byteOffset: number) {
    if (!view) return;
    const b2u = makeByteToU16(view.state.doc.toString());
    scrollToOffset(b2u(byteOffset));
  }

  /** Replace the buffer range `[startByte, endByte)` (UTF-8 byte offsets, as backend edits report
   *  them) with `text`, then place the caret at the end of the insertion and focus. Byte offsets
   *  are mapped through `makeByteToU16` against the live buffer. The dispatch emits a normal doc
   *  change, so the host's controlled-value sync + live re-index pick it up. No-op when unmounted. */
  export function replaceByteRange(startByte: number, endByte: number, text: string) {
    if (!view) return;
    const b2u = makeByteToU16(view.state.doc.toString());
    const len = view.state.doc.length;
    const from = Math.max(0, Math.min(b2u(startByte), len));
    const to = Math.max(from, Math.min(b2u(endByte), len));
    view.dispatch({
      changes: { from, to, insert: text },
      selection: { anchor: from + text.length },
    });
    view.focus();
  }

  /**
   * Replace several byte ranges at once, as **one** edit.
   *
   * Not a loop over {@link replaceByteRange}, and the difference is the whole
   * reason this exists: applied one at a time, each replacement shifts the offsets
   * of the ones after it (so they would all have to be applied backwards to be
   * correct at all), and each becomes its own undo step — so taking back a
   * forty-match structural replace would mean forty presses of Ctrl+Z.
   *
   * CodeMirror reads an array of changes against the *starting* document, which is
   * exactly how the ranges are expressed, so they are dispatched together and
   * undone together. Ranges are sorted and any that overlap a previous one is
   * dropped: a change set with overlapping edits throws, and a caller sending one
   * should get the edit it could have, not an exception. Returns how many landed.
   */
  export function replaceByteRanges(
    edits: readonly { startByte: number; endByte: number; text: string }[],
  ): number {
    if (!view || !edits.length) return 0;
    const b2u = makeByteToU16(view.state.doc.toString());
    const len = view.state.doc.length;

    const mapped = edits
      .map((e) => {
        const from = Math.max(0, Math.min(b2u(e.startByte), len));
        return { from, to: Math.max(from, Math.min(b2u(e.endByte), len)), insert: e.text };
      })
      .sort((a, b) => a.from - b.from);

    const changes: typeof mapped = [];
    let consumed = -1;
    for (const change of mapped) {
      if (change.from < consumed) continue;
      changes.push(change);
      consumed = change.to;
    }
    if (!changes.length) return 0;

    view.dispatch({ changes });
    view.focus();
    return changes.length;
  }

  export function scrollToLineCol(line: number, col = 1) {
    if (!view) return;
    const doc = view.state.doc;
    const ln = Math.max(1, Math.min(line, doc.lines));
    const lineInfo = doc.line(ln);
    const pos = Math.min(lineInfo.from + Math.max(0, col - 1), lineInfo.to);
    view.dispatch({
      selection: { anchor: pos },
      effects: EditorView.scrollIntoView(pos, { y: 'center' }),
    });
    view.focus();
  }

  /**
   * Bring a line into view **without taking the focus**, and without moving the caret.
   *
   * The counterpart of {@link scrollToLineCol}, which exists to *go* somewhere and therefore
   * ends by focusing. This one is for an editor the user is **looking at rather than working
   * in** — a preview beside a search field. There, focusing is not a detail: every arrow key
   * would pull the caret out of the field being typed into, and the field is the whole point.
   *
   * No selection change either: a selection the user did not make is a selection they then have
   * to undo if they do click in.
   */
  export function revealLine(line: number) {
    if (!view) return;
    const doc = view.state.doc;
    const ln = Math.max(1, Math.min(line, doc.lines));
    view.dispatch({ effects: EditorView.scrollIntoView(doc.line(ln).from, { y: 'center' }) });
  }

  /** Imperatively replace the diagnostics (byte spans → lint), e.g. after a fresh
   *  async lint run when the host isn't binding the `diagnostics` prop. */
  export function setDiagnostics(errors: EditorDiagnostic[]) {
    if (!view) return;
    const src = view.state.doc.toString();
    view.dispatch(cmSetDiagnostics(view.state, toCmDiagnostics(errors, src)));
  }

  /** The caret's viewport coordinates (bottom-left of the primary selection head),
   *  for anchoring a caret-attached popup (intentions / usages). Null when the
   *  editor isn't mounted or the position is off-screen. Mirrors merula's
   *  `anchorAt`. */
  export function coordsAtCaret(): { x: number; y: number } | null {
    if (!view) return null;
    const c = view.coordsAtPos(view.state.selection.main.head);
    return c ? { x: c.left, y: c.bottom } : null;
  }

  /** Move the caret to the document position under viewport coords (`x`, `y`) — used to
   *  position the caret on a right-click before a context-menu action runs (a right-click
   *  doesn't move the caret on its own, so caret-based actions like go-to-declaration /
   *  rename / find-usages would otherwise target the OLD caret, not what was clicked).
   *  Returns true when a position was found under the point. */
  export function setCaretAtCoords(x: number, y: number): boolean {
    if (!view) return false;
    const pos = view.posAtCoords({ x, y });
    if (pos == null) return false;
    // Don't collapse a non-empty selection when the click lands INSIDE it — so a
    // right-click-then-Copy/Cut still operates on the selection. Only move the caret when
    // clicking outside any selection (IntelliJ / browser behaviour).
    const sel = view.state.selection.main;
    if (!sel.empty && pos >= sel.from && pos <= sel.to) return true;
    view.dispatch({ selection: { anchor: pos } });
    return true;
  }

  /** Viewport coords (bottom-left) for a **UTF-8 byte offset** rather than the caret — for
   *  anchoring a popup at a position the host knows: a Ctrl+Click that falls back to
   *  find-usages, or a keyboard go-to that resolved to several places and has to ask which,
   *  with no pointer to anchor to. Null when unmounted or the position is off-screen. */
  export function coordsAtByteOffset(byteOffset: number): { x: number; y: number } | null {
    if (!view) return null;
    const b2u = makeByteToU16(view.state.doc.toString());
    const pos = Math.max(0, Math.min(b2u(byteOffset), view.state.doc.length));
    const c = view.coordsAtPos(pos);
    return c ? { x: c.left, y: c.bottom } : null;
  }

  /** The identifier under (or just before) the caret, or null. Boundary-tolerant:
   *  the caret often sits at a word's right edge, so we scan both directions from
   *  the head. Used to label context actions (e.g. "Add import for <word>"). */
  export function wordAtCaret(): string | null {
    if (!view) return null;
    const doc = view.state.doc;
    const head = view.state.selection.main.head;
    const line = doc.lineAt(head);
    const text = line.text;
    const rel = head - line.from;
    const isWord = (ch: string) => /[A-Za-z0-9_$]/.test(ch);
    // Expand left/right from the caret (checking the char before too, so `foo|`
    // resolves to `foo`).
    let start = rel;
    let end = rel;
    while (start > 0 && isWord(text[start - 1])) start--;
    while (end < text.length && isWord(text[end])) end++;
    const word = text.slice(start, end);
    return word.length ? word : null;
  }

  /** The reference-like token at the caret — a string-literal's contents (a JSP
   *  `action="…"` value) or a path/identifier run (`/do/Category/viewTree`), or null.
   *  Powers a host go-to-definition triggered by keyboard (vs the Ctrl+Click seam,
   *  which resolves the same token at the click position via `onGoto`). */
  export function refAtCaret(): string | null {
    if (!view) return null;
    return refTextAt(view.state.doc, view.state.selection.main.head);
  }

  /** The current selection's text, or '' when nothing is selected — to seed a search /
   *  navigator field from what the user highlighted (IntelliJ / VS Code). */
  export function getSelectionText(): string {
    if (!view) return '';
    const s = view.state.selection.main;
    return view.state.sliceDoc(s.from, s.to);
  }

  /** Where the caret is and what, if anything, is selected — in CodeMirror positions
   *  (UTF-16 code units). `empty` distinguishes "the user highlighted a range" from
   *  "the caret happens to sit somewhere", which is a different instruction for any
   *  command that acts on a region. Zeroes when unmounted. */
  export function selectionRange(): { from: number; to: number; head: number; empty: boolean } {
    if (!view) return { from: 0, to: 0, head: 0, empty: true };
    const s = view.state.selection.main;
    return { from: s.from, to: s.to, head: s.head, empty: s.empty };
  }

  /** Highlight `[from, to)` and scroll it into view — how a command shows *which*
   *  region it is about to act on, in the buffer rather than in a message. */
  export function selectRange(from: number, to: number) {
    if (!view) return;
    const len = view.state.doc.length;
    const anchor = Math.max(0, Math.min(from, len));
    const head = Math.max(anchor, Math.min(to, len));
    view.dispatch({
      selection: { anchor, head },
      effects: EditorView.scrollIntoView(anchor, { y: 'nearest' }),
    });
  }

  /** Select `[startByte, endByte)` — the byte-aware sibling of {@link selectRange}.
   *
   *  Everything a backend hands back is in UTF-8 bytes (a syntax node's range, a
   *  structural match), and every one of those offsets is wrong by the number of
   *  non-ASCII characters before it if it reaches CodeMirror unconverted. Which is
   *  every accented comment in the files this is pointed at, so the conversion is
   *  not an edge case and does not belong at the call site. */
  export function selectByteRange(startByte: number, endByte: number) {
    if (!view) return;
    const b2u = makeByteToU16(view.state.doc.toString());
    selectRange(b2u(startByte), b2u(endByte));
  }

  /** Copy the current selection to the clipboard (no-op when nothing is selected). */
  export function copySelection() {
    if (!view) return;
    const s = view.state.selection.main;
    const text = view.state.sliceDoc(s.from, s.to);
    if (text) void navigator.clipboard.writeText(text).catch(() => {});
  }

  /** Cut the current selection to the clipboard (no-op when nothing is selected). */
  export function cutSelection() {
    if (!view) return;
    const s = view.state.selection.main;
    const text = view.state.sliceDoc(s.from, s.to);
    if (!text) return;
    void navigator.clipboard.writeText(text).catch(() => {});
    view.dispatch({ changes: { from: s.from, to: s.to, insert: '' } });
    view.focus();
  }

  /** Paste clipboard text at the caret (replacing any selection). Best-effort — a
   *  blocked clipboard read is swallowed. */
  export async function pasteClipboard() {
    if (!view) return;
    let text = '';
    try { text = await navigator.clipboard.readText(); } catch { return; }
    if (!text) return;
    const s = view.state.selection.main;
    view.dispatch({
      changes: { from: s.from, to: s.to, insert: text },
      selection: { anchor: s.from + text.length },
    });
    view.focus();
  }

  /** Insert `text` at the caret (replacing any selection), leaving the caret right
   *  after the inserted text. Used by generator flows (Alt+Insert → Generate).
   *  Mirrors merula's `insertAtCursor`. */
  export function insertAtCursor(text: string) {
    if (!view || !text) return;
    const sel = view.state.selection.main;
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert: text },
      selection: { anchor: sel.from + text.length },
    });
    view.focus();
  }
</script>

<!-- CodeMirror mount host: the editable surface and all keyboard interaction live in
     CM inside this node. -->
<div class="code-editor" bind:this={hostEl}></div>

<style>
  .code-editor {
    flex: 1;
    min-width: 0; min-height: 0;
    background: var(--bg-base);
    overflow: hidden;
  }
  .code-editor :global(.cm-editor) { height: 100%; }

  /* Host gutter icons. Scoped under `.code-editor` so this stays a component rule
     despite `:global` — the classes are on nodes CodeMirror creates, which Svelte's
     scoping hash never reaches. No padding on the empty column: an editor whose host
     passes no marks must not pay horizontal space for a gutter it never uses. */
  .code-editor :global(.cm-host-gutter) { min-width: 0; }
  .code-editor :global(.cm-host-gutter .cm-gutterElement) { padding: 0; }
  .code-editor :global(.cm-host-gutter-icon) {
    display: inline-flex; align-items: center; justify-content: center;
    width: 15px; height: 100%;
    font-size: 10px; line-height: 1;
    color: var(--text-muted); cursor: pointer;
    transition: color var(--transition-fast);
  }
  .code-editor :global(.cm-host-gutter-icon:hover) { color: var(--accent); }

  /* The flag gutter (breakpoints). Every line carries a marker, set or not, because an
     unset line still has to say it can be clicked — which it does by showing a faint dot
     under the pointer and nothing at all otherwise. */
  .code-editor :global(.cm-flag-gutter) { min-width: 0; cursor: pointer; }
  .code-editor :global(.cm-flag-gutter .cm-gutterElement) {
    padding: 0 3px;
    display: flex; align-items: center; justify-content: center;
  }
  /* `display: block` explicitly: a `<span>` is inline, and an inline box ignores width and
     height — which would leave a correctly-classed marker rendering nothing at all. */
  .code-editor :global(.cm-flag-icon) {
    display: block;
    width: 10px; height: 10px; border-radius: 50%;
    background: var(--error);
    opacity: 0;
    transition: opacity var(--transition-fast);
  }
  .code-editor :global(.cm-flag-gutter .cm-gutterElement:hover .cm-flag-icon) { opacity: 0.35; }
  .code-editor :global(.cm-flag-icon.cm-flag-on) { opacity: 1; }
  .code-editor :global(.cm-flag-gutter .cm-gutterElement:hover .cm-flag-icon.cm-flag-on) {
    opacity: 0.7;
  }
</style>
