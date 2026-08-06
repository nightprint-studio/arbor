/**
 * The two trees of whatever file is in front of the user.
 *
 * ## Two readings, one file
 *
 * * **Syntax** — the parse. Every node the grammar built, punctuation included. It answers *why
 *   did it read my file that way*.
 * * **Model** — the AST. The same parse in Java's vocabulary and all the way down: statements,
 *   expressions, and the type each one resolved to. It is what the index, completion, the checks
 *   and go-to all reason over, so it is the one that explains a wrong answer.
 *
 * Both arrive in the same shape, so the panel that draws them is the same panel and the choice
 * is a tab. Both are fetched only when their tab is shown: the model of a large legacy file is
 * cheap, but it is not free, and nobody is reading a tab they cannot see.
 *
 * ## They follow the buffer, not the file
 *
 * A tree of what is *on disk* while you edit would be wrong from the first keystroke, and wrong
 * in the way that matters most — the moment you want the tree is the moment you have typed
 * something that read differently than you expected. So the text comes from the editor,
 * debounced.
 *
 * ## Selection travels both ways
 *
 * Clicking a node selects its bytes in the editor; moving the caret reveals the node holding it.
 * The second half is what makes the panel a reading tool rather than a toy: you point at the
 * construct you do not understand and the panel says what it is.
 *
 * Only the **visible** view is revealed. The caret moves on every arrow key, and revealing a tab
 * nobody can see would spend a round trip per keystroke on nothing; the offset is kept instead,
 * and the other tab catches up the moment it is picked — so switching still lands where you were
 * rather than at the top.
 *
 * The two views answer "what am I in" differently, and both are the cheap way for their own
 * shape: the parse tree may have been **truncated**, so it asks the backend — anything else
 * would make the answer depend on how far the walk happened to go. The model tree never is, so
 * it walks what it already holds.
 *
 * ## A language with no grammar — or no model — is not an error
 *
 * Bennu edits Java, XML, JSP, properties. It parses Java and models Java. So each view keeps the
 * language it could not read, and says so in its own words: "no grammar for XML" and "no model
 * for XML" are different claims, and a panel that used one wording for both would be wrong on
 * one of the tabs.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { untrack } from 'svelte';

import { symbolTreeOf, syntaxPathAt, syntaxTreeOf } from '$lib/ipc/bennu/ast';
import type { SyntaxTreeSource, SyntaxTreeTab } from '$lib/components/shared/internal/SyntaxTreePanel.svelte';
import { nodeKey, type SyntaxNode, type SyntaxTree } from '$lib/types/syntax';

/** How long a buffer must sit still before it is re-read. */
const DEBOUNCE_MS = 220;

export type AstViewId = 'syntax' | 'model';

/** The strip the panel draws. Exported so the shell does not re-state the labels. */
export const AST_VIEWS: SyntaxTreeTab[] = [
  {
    id: 'syntax',
    label: 'Syntax',
    hint: 'What the grammar built — every node, punctuation included. Why the file read the way it did.',
  },
  {
    id: 'model',
    label: 'Model',
    hint: 'The AST — the same parse in Java’s words, bodies included, with the type each expression resolved to.',
  },
];

/** Everything one view holds. Deeply reactive, so a field assignment is enough. */
interface ViewState {
  tree: SyntaxTree | null;
  loading: boolean;
  error: string | null;
  /** The language, when there is one and this view cannot read it. */
  unparsed: string | null;
  revealed: string[];
  selectedKey: string | null;
  /** The revision this view's tree was built for — how it knows it is stale. */
  of: number;
}

function blank(): ViewState {
  return { tree: null, loading: false, error: null, unparsed: null, revealed: [], selectedKey: null, of: -1 };
}

function createBennuAstStore() {
  const syntax = $state<ViewState>(blank());
  const model = $state<ViewState>(blank());
  const states: Record<AstViewId, ViewState> = { syntax, model };

  let active = $state<AstViewId>('syntax');
  /** The buffer both views describe — what a click's ranges index into. */
  let text = $state('');
  /** The file. Held because it picks the grammar, so a refetch has to send it again. */
  let path = $state('');
  let namedOnly = $state(false);
  /**
   * Where the caret last was, in bytes.
   *
   * Held rather than acted on twice: only the visible view is revealed, so the other one needs
   * somewhere to read the offset it slept through when its tab is picked.
   */
  let caret: number | null = null;
  /** Bumped when a node is clicked; the view watches it and selects in the editor. */
  let selectRequest = $state<{ start: number; end: number; at: number } | null>(null);

  /** Guards against an older answer landing after a newer one. Per view: they race separately. */
  const seq: Record<AstViewId, number> = { syntax: 0, model: 0 };
  let timer: ReturnType<typeof setTimeout> | null = null;

  /**
   * What each view *should* be showing.
   *
   * A revision counter rather than the buffer itself: staleness is asked on every keystroke and
   * on every tab switch, and comparing a megabyte of source each time to learn "still the same"
   * is work for nothing. Bumped per view, because flipping the punctuation filter invalidates
   * the parse tree and leaves the model's alone.
   */
  const want: Record<AstViewId, number> = { syntax: 0, model: 0 };

  async function load(id: AstViewId) {
    const state = states[id];
    const wanted = want[id];
    if (state.of === wanted) return;
    if (!text && !path) return;

    const mine = ++seq[id];
    state.loading = true;
    try {
      const answer =
        id === 'syntax'
          ? await syntaxTreeOf(text, path, { namedOnly })
          : await symbolTreeOf(text, path);
      if (mine !== seq[id]) return;
      state.tree = answer.tree;
      state.unparsed = answer.tree ? null : answer.language;
      state.error = null;
      state.of = wanted;
    } catch (e) {
      if (mine !== seq[id]) return;
      state.tree = null;
      state.unparsed = null;
      state.error = String(e);
      // Left stale on purpose: a failed read must be retried, not remembered as an answer.
      state.of = -1;
    } finally {
      if (mine === seq[id]) state.loading = false;
    }
  }

  /** Ranges of every node containing `offset`, outermost first — the model view's own reveal. */
  function pathIn(node: SyntaxNode, offset: number, out: string[]): string[] {
    if (offset < node.range.start || offset > node.range.end) return out;
    out.push(`${node.range.start}:${node.range.end}`);
    for (const child of node.children ?? []) pathIn(child, offset, out);
    return out;
  }

  /**
   * Open one view down to wherever the caret last was.
   *
   * Called for the **visible** view only — on a caret move, and again when a tab is picked, so
   * the one arriving catches up on the offset it slept through. Each by the means that is right
   * for its shape: the parse tree may have been truncated, so it asks the backend rather than
   * letting the answer depend on how far the walk went; the AST never is, so it walks what it
   * already holds and costs nothing.
   */
  async function revealIn(id: AstViewId) {
    if (caret === null || !text) return;
    const state = states[id];
    if (id === 'model') {
      if (!state.tree) return;
      state.revealed = pathIn(state.tree.root, caret, []);
      state.selectedKey = null;
      return;
    }
    const mine = seq.syntax;
    const at = caret;
    try {
      const found = await syntaxPathAt(text, path, at);
      // Both guards matter: a newer parse, or a newer caret whose own reveal is already in
      // flight — landing an older path would jump the tree away from where you are looking.
      if (mine !== seq.syntax || at !== caret) return;
      state.revealed = found.map((r) => `${r.start}:${r.end}`);
      // Revealed without a select request: the caret is already there, and moving the selection
      // under the user's own caret would fight them.
      state.selectedKey = null;
    } catch {
      // A path that cannot be computed is not worth a message: the tree itself is on screen and
      // still readable.
    }
  }

  /**
   * The adapter the panel consumes for one view.
   *
   * A stable object with getters rather than a fresh literal per read: a rune store's
   * reactivity lives in its getters, and spreading one into a plain object would hand the panel
   * a snapshot that never changes again.
   */
  function sourceFor(id: AstViewId): SyntaxTreeSource {
    const state = states[id];
    const isSyntax = id === 'syntax';
    const view: SyntaxTreeSource = {
      get tree() { return state.tree; },
      get loading() { return state.loading; },
      get error() { return state.error; },
      get unparsedLanguage() { return state.unparsed; },
      get revealed() { return state.revealed; },
      get selectedKey() { return state.selectedKey; },
      get namedOnly() { return namedOnly; },
      unavailableTemplate: isSyntax
        ? 'No grammar for {language} yet — nothing to draw a tree from.'
        : 'Bennu has no declaration model for {language} yet — it reads and edits it, but does not model it.',
      select(node: SyntaxNode) {
        state.selectedKey = nodeKey(node);
        selectRequest = { ...node.range, at: Date.now() };
      },
    };
    // Attached rather than spread in, because spreading an object literal *invokes* its getters
    // and would hand the panel a frozen snapshot. Only the parse has anonymous nodes to hide, so
    // on the model tab the toggle is absent rather than present and inert.
    if (isSyntax) {
      view.setNamedOnly = (yes: boolean) => {
        if (yes === namedOnly) return;
        namedOnly = yes;
        // A different tree, not a stale one.
        want.syntax++;
        void untrack(() => load('syntax'));
      };
    }
    return view;
  }

  const sources: Record<AstViewId, SyntaxTreeSource> = {
    syntax: sourceFor('syntax'),
    model: sourceFor('model'),
  };

  return {
    get views() { return AST_VIEWS; },
    get activeView() { return active; },
    /** The source for the tab being shown — what the panel is handed. */
    get source() { return sources[active]; },
    get text() { return text; },
    get selectRequest() { return selectRequest; },

    setActiveView(id: string) {
      if (id !== 'syntax' && id !== 'model') return;
      active = id;
      untrack(() => {
        // Both on arrival, not on every keystroke: nobody is reading a tab they cannot see.
        // The reveal comes second so it runs against the tree the load is about to settle.
        void load(active).then(() => revealIn(active));
      });
    },

    /**
     * The buffer changed. Debounced, and only the visible view is refreshed — the other one is
     * left marked stale and reloads when its tab is picked.
     */
    follow(source: string, file: string) {
      if (source === text && file === path) return;
      text = source;
      path = file;
      want.syntax++;
      want.model++;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => void load(active), DEBOUNCE_MS);
    },

    /** The document went away. */
    clear() {
      if (timer) clearTimeout(timer);
      for (const id of ['syntax', 'model'] as AstViewId[]) {
        seq[id]++;
        want[id]++;
        Object.assign(states[id], blank());
      }
      text = '';
      path = '';
      caret = null;
    },

    /**
     * The caret moved: open the tree down to what holds it.
     *
     * **The visible one only.** Revealing in the hidden tab would cost a round trip per caret
     * move for something nobody can see — the caret moves on every arrow key. The offset is kept
     * instead, and the other tab catches up the moment it is picked, so switching still lands
     * where you were rather than at the top.
     */
    async revealAt(offset: number) {
      caret = offset;
      await revealIn(active);
    },
  };
}

export const bennuAstStore = createBennuAstStore();
