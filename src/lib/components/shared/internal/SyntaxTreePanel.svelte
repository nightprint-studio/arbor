<script lang="ts" module>
  import type { SyntaxNode } from '$lib/types/syntax';

  /**
   * What this panel needs of a host's syntax-tree store.
   *
   * A prop rather than an import, because two products have one of these — Picus over SQL,
   * Bennu over Java — and the panel is the same panel. What differs is which backend answers,
   * which is precisely what a store already encapsulates.
   */
  export interface SyntaxTreeSource {
    /** The tree, or `null` when there is nothing to show yet. */
    readonly tree: { root: SyntaxNode; nodeCount: number; truncated: boolean; hasErrors: boolean } | null;
    readonly loading: boolean;
    /** A real failure — the parse blew up. Not the same as "no grammar", see {@link language}. */
    readonly error: string | null;
    /** Ranges of the nodes to draw open, as `start:end`. */
    readonly revealed: string[];
    readonly selectedKey: string | null;
    /**
     * The language of the document, when the host can name one it has no grammar for. The panel
     * then says so instead of showing an empty tree — "no grammar for XML yet" is a statement
     * about the tool, and a blank panel would be an implied statement about the file.
     */
    readonly unparsedLanguage?: string | null;
    /**
     * What to say when {@link unparsedLanguage} is set. A template with `{language}` in it, so a
     * host whose view is not a *parse* can say why in its own words — "no model for XML yet"
     * rather than "no grammar", which would be a different and untrue claim.
     */
    readonly unavailableTemplate?: string;
    /**
     * The punctuation toggle, when this view has one. A derived view has no anonymous nodes to
     * hide, and the button is absent there rather than present and inert.
     */
    readonly namedOnly?: boolean;
    setNamedOnly?(yes: boolean): void;
    select(node: SyntaxNode): void;
  }

  /** One entry in the panel's tab strip. */
  export interface SyntaxTreeTab {
    id: string;
    label: string;
    /** Hover text — the tab labels are short, and what the two views mean is not obvious. */
    hint?: string;
  }
</script>

<script lang="ts">
  /**
   * The syntax tree of the document in front of the user.
   *
   * It answers one question, and the whole panel is arranged around it: **why did the parser
   * read it that way?** Which is why the anonymous nodes — the commas, the keywords — are shown
   * by default. They are noisy, and they are very often the answer; the toggle is there for the
   * reading where they are not.
   *
   * A click selects the node's bytes in the editor. Moving the caret opens the tree down to the
   * node holding it. Both directions matter: one is "show me what this is", the other is "show
   * me where that is".
   *
   * **Shared, not Picus's.** The walk behind it is `arbor-syntax`, which knows no language, and
   * the shape it produces is the same whichever backend built it — so a second copy of this
   * panel for Bennu's Java trees would have been a second place to fix every one of the
   * decisions above. The host supplies a {@link SyntaxTreeSource}; everything else is here.
   *
   * **More than one reading of the same file.** A host can offer several trees — Bennu shows the
   * parse and the declaration model it derives from it — through {@link Props.tabs}. The panel
   * only draws the strip and reports the click; the host swaps `source`. That is what keeps this
   * component at "draw one tree" while the second view's fetching and staleness stay in the store
   * that owns them, and it is why Picus, which has one tree, passes no tabs and changes nothing.
   */
  import { Braces, TriangleAlert, Filter as FilterIcon } from 'lucide-svelte';
  import PanelShell from '../ui/PanelShell.svelte';
  import Tree from '../ui/Tree.svelte';
  import Button from '../ui/Button.svelte';
  import Badge from '../ui/Badge.svelte';
  import Spinner from '../ui/Spinner.svelte';
  import StateBlock from '../ui/StateBlock.svelte';
  import Alert from '../ui/Alert.svelte';
  import SearchBar from '../ui/SearchBar.svelte';

  interface Props {
    /** The host's store for the view being shown — see {@link SyntaxTreeSource}. */
    source: SyntaxTreeSource;
    /** Shown when there is no document at all. */
    emptyMessage?: string;
    /** Panel title. A host with more than one view usually wants a name covering both. */
    title?: string;
    /**
     * The views this host offers, when there is more than one. Absent — or a single entry — draws
     * no strip at all: a tab bar over one tab is a control that cannot be used.
     *
     * The panel does **not** hold the views; the host swaps `source` when a tab is picked. That
     * keeps this component's contract at "draw one tree" and leaves the second view's fetching,
     * caching and staleness where they belong.
     */
    tabs?: SyntaxTreeTab[];
    activeTab?: string;
    onTab?: (id: string) => void;
  }

  let {
    source,
    emptyMessage = 'Open a file and its syntax tree appears here.',
    title = 'Syntax tree',
    tabs,
    activeTab,
    onTab,
  }: Props = $props();

  let filter = $state('');
  const strip = $derived((tabs ?? []).length > 1 ? tabs! : []);

  const tree = $derived(source.tree);
  /** The root is drawn as a row too — it is a node, and its range is the file. */
  const nodes = $derived<SyntaxNode[]>(tree ? [tree.root] : []);

  /** Identity: the range plus the kind. Unique inside one tree, stable across re-parses that
   *  did not touch this node — so expansion survives typing. */
  function idOf(node: SyntaxNode): string {
    return `${node.range.start}:${node.range.end}:${node.kind}`;
  }

  const expanded = $derived(new Set(source.revealed.flatMap((r) => {
    // The store keeps `start:end`; the tree keys include the kind, so a revealed range opens
    // whichever node holds it.
    return nodes.length ? idsMatching(nodes[0], r) : [];
  })));

  function idsMatching(node: SyntaxNode, range: string): string[] {
    const own = `${node.range.start}:${node.range.end}` === range ? [idOf(node)] : [];
    return own.concat((node.children ?? []).flatMap((c) => idsMatching(c, range)));
  }

  /** Locally toggled nodes, on top of whatever the caret revealed. */
  let opened = $state(new Set<string>());
  const expandedIds = $derived(new Set([...expanded, ...opened]));

  // ── following the caret all the way ──────────────────────────────────────────
  /**
   * The deepest revealed node — the one the caret is actually in.
   *
   * `revealed` is root-to-leaf, so its last entry is the innermost range. Several nodes can
   * share that range (tree-sitter wraps an `identifier` in an `expression` in a `statement`
   * with the same bytes); `idsMatching` walks own-then-children, so the last id it yields is
   * the deepest of them, which is the one worth landing on.
   */
  const revealedLeaf = $derived.by(() => {
    const range = source.revealed[source.revealed.length - 1];
    if (!range || !nodes.length) return null;
    const ids = idsMatching(nodes[0], range);
    return ids[ids.length - 1] ?? null;
  });

  let treeView = $state<{ scrollToId: (id: string, block?: 'center' | 'nearest') => void } | null>(null);
  /** The last node scrolled to, so typing inside one does not re-scroll on every keystroke —
   *  and so scrolling away by hand, with the caret where it was, stays where you put it. */
  let scrolledTo = '';

  /**
   * Opening the tree down to a node is only half of "show me where the caret is": if that node
   * is below the fold, nothing visible happened at all. So it is scrolled to as well.
   *
   * Through the Tree's own `scrollToId` rather than `scrollIntoView`, because the Tree is
   * **virtualised** — a row that is off-screen is not in the DOM, which is exactly the case
   * this fixes, so there is no element to scroll to. It computes the position from the row's
   * index instead.
   *
   * `nearest`, not `center`: the node is usually already on screen, and a tree that re-centred
   * itself on every caret move would be unusable to read.
   *
   * On a frame, because `scrollToId` needs two things that are not ready in this one — the
   * newly expanded rows in the flat list, and a measured viewport.
   */
  $effect(() => {
    const id = revealedLeaf;
    if (!id || id === scrolledTo) return;
    scrolledTo = id;
    const frame = requestAnimationFrame(() => treeView?.scrollToId(id, 'nearest'));
    return () => cancelAnimationFrame(frame);
  });

  function toggle(id: string, next: boolean) {
    const copy = new Set(opened);
    if (next) copy.add(id);
    else copy.delete(id);
    opened = copy;
  }

  /** Matching on the kind AND the text: "the identifier called ETICHETTA" is the search
   *  somebody actually runs, and neither half alone finds it. */
  function match(node: SyntaxNode, q: string): boolean {
    const needle = q.toLowerCase();
    return (
      node.kind.toLowerCase().includes(needle) ||
      (node.field ?? '').toLowerCase().includes(needle) ||
      (node.text ?? '').toLowerCase().includes(needle)
    );
  }
</script>

<PanelShell {title}>
  {#snippet icon()}<Braces size={13} />{/snippet}

  {#snippet actions()}
    {#if source.loading}<Spinner size={11} />{/if}
    {#if source.setNamedOnly}
      <Button
        variant="icon"
        size="xs"
        ariaLabel={source.namedOnly ? 'Show every node' : 'Hide punctuation and keywords'}
        tooltip={source.namedOnly
          ? 'Showing only the grammar’s own concepts — click for every node, punctuation included'
          : 'Showing every node. The commas and the keywords are often the reason a file reads oddly — click to hide them anyway'}
        onclick={() => source.setNamedOnly?.(!source.namedOnly)}
      >
        {#snippet iconStart()}<FilterIcon size={13} />{/snippet}
      </Button>
    {/if}
  {/snippet}

  {#if strip.length}
    <div class="ap-tabs" role="tablist" aria-label="Which tree to show">
      {#each strip as tab (tab.id)}
        <button
          type="button"
          role="tab"
          class="ap-tab"
          class:ap-on={tab.id === activeTab}
          aria-selected={tab.id === activeTab}
          title={tab.hint}
          onclick={() => onTab?.(tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>
  {/if}

  {#if source.error}
    <div class="ap-pad">
      <Alert variant="error" compact title="The tree could not be built" text={source.error} />
    </div>
  {:else if !tree && source.unparsedLanguage}
    <!-- A statement about the tool, not about the file: this language is readable and editable,
         and Bennu simply has nothing that reads it yet. The wording is the host's, because
         "no grammar" and "no model" are different claims and only one of them is true per view. -->
    <StateBlock
      tone="info"
      label={(source.unavailableTemplate ??
        'No grammar for {language} yet — nothing to draw a tree from.'
      ).replace('{language}', source.unparsedLanguage)}
    />
  {:else if !tree}
    <StateBlock tone="info" label={source.loading ? 'Reading the document…' : emptyMessage} />
  {:else}
    <div class="ap-head">
      <SearchBar
        query={filter}
        showRegex={false}
        showCounter={false}
        placeholder="Filter by kind, field or text"
        oninput={(v: string) => (filter = v)}
      />
      <div class="ap-stats">
        <Badge variant="tone" tone="neutral" size="sm" label={`${tree.nodeCount} nodes`} />
        {#if tree.hasErrors}
          <Badge variant="tone" tone="warning" size="sm" label="parse errors" />
        {/if}
        {#if tree.truncated}
          <!-- Said, never implied: a tree that stopped early and a file that simply ends here
               look identical otherwise. -->
          <Badge variant="tone" tone="info" size="sm" label="truncated" />
        {/if}
      </div>
    </div>

    <div class="ap-tree">
      <Tree
        bind:this={treeView}
        {nodes}
        getId={idOf}
        getChildren={(n) => n.children}
        hasChildren={(n) => !!(n.children?.length || n.elided)}
        expandedIds={expandedIds}
        onExpandToggle={(id, next) => toggle(id, next)}
        selectedId={source.selectedKey}
        {filter}
        {match}
        guides
        rowHeight={20}
        onSelect={(node) => source.select(node)}
      >
        {#snippet row({ node })}
          <span class="ap-row" class:ap-error={node.error || node.missing}>
            {#if node.field}
              <!-- The single most useful column. On a parse tree it is the grammar's field —
                   the difference between "an identifier" and "the table being written to"; on a
                   derived tree it is the modifiers, which play the same role: what makes this
                   row different from the one under it. -->
              <span class="ap-field">{node.field}</span>
            {/if}
            <span class="ap-kind" class:ap-anon={!node.named}>{node.kind}</span>
            {#if node.synthesized}
              <!-- Nobody wrote it. Said, never implied — its range points at whatever declares
                   it, and without this the panel would be claiming those bytes are the member. -->
              <Badge variant="tone" tone="neutral" size="sm" label="generated" />
            {/if}
            {#if node.missing}
              <Badge variant="tone" tone="warning" size="sm" label="invented" />
            {:else if node.error}
              <TriangleAlert size={11} class="ap-warn" />
            {/if}
            {#if node.injected}
              <!-- Its children came from a second parse: the SQL grammar hands a `$$ … $$`
                   routine body back as one token, and that is where an update script does its
                   work. Worth marking — it is the difference between "the grammar reads this"
                   and "we read this separately because the grammar would not". -->
              <Badge variant="tone" tone="info" size="sm" label="body" />
            {/if}
            {#if node.text}<span class="ap-text">{node.text}</span>{/if}
            {#if node.elided}<span class="ap-more">…</span>{/if}
          </span>
        {/snippet}
      </Tree>
    </div>
  {/if}
</PanelShell>

<style>
  .ap-pad { padding: 8px; }

  /* The view strip. Flush under the header and on the panel's own surface, so the two trees read
     as two readings of one thing rather than as two panels sharing a slot. */
  .ap-tabs {
    display: flex;
    gap: 2px;
    padding: 4px 6px 0 6px;
  }
  .ap-tab {
    appearance: none;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    padding: 3px 8px 4px 8px;
    font: inherit;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
  }
  .ap-tab:hover { color: var(--text-secondary); background: var(--bg-hover); }
  .ap-tab.ap-on {
    color: var(--text-primary);
    border-bottom-color: var(--accent);
  }

  .ap-head {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .ap-stats { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }

  .ap-tree { overflow: auto; flex: 1; min-height: 0; }

  .ap-row {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    font-size: var(--font-size-xs);
  }
  .ap-row.ap-error { color: var(--error); }

  .ap-field {
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--accent);
    flex-shrink: 0;
  }
  .ap-kind {
    font-family: var(--font-code);
    color: var(--text-primary);
    flex-shrink: 0;
  }
  /* Anonymous nodes are the punctuation and the keywords: present, and visibly secondary to the
     grammar's own concepts. */
  .ap-kind.ap-anon { color: var(--text-muted); font-style: italic; }

  .ap-text {
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    opacity: 0.8;
  }
  .ap-more { color: var(--text-muted); }
</style>
