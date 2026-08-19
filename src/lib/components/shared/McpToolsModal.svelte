<!--
  What Arbor exposes to an AI client, program by program.

  A reference, not a setting: nothing here changes anything, and everything listed is
  already reachable by a connected client. It is worth a window of its own because the same
  list is otherwise readable only *by the model* — a user who wants to know what an
  assistant can do with their machine would have to ask the assistant.

  ## Why master–detail and not a list

  A tool's description is written for a model deciding whether to call it, so it is a
  paragraph, not a label. Twenty-one paragraphs stacked in a column is a document nobody
  reads: you cannot see how many tools there are, you cannot compare two, and finding one
  means scrolling past the prose of every tool before it. So the left column answers "what
  is there" at a glance and the right one answers "what does this one do" — the shape
  IntelliJ uses for exactly this problem, and the reason the description can stay in full.

  Programs that are switched off are listed too, with their tools intact and a line saying
  so. Deciding whether to expose a product is a decision about what its tools would let an
  assistant do, and a list that only appears once you have exposed it is no help in making
  it.
-->
<script lang="ts">
  import { Bot } from 'lucide-svelte';

  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ChipBar from '$lib/components/shared/ui/ChipBar.svelte';
  import type { ChipItem } from '$lib/components/shared/ui/ChipBar.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { getMcpTools } from '$lib/ipc/mcp';
  import { MCP_PRODUCTS } from '$lib/types/mcp';
  import type { McpProgramTools, McpToolSummary } from '$lib/types/mcp';

  let { onClose }: { onClose: () => void } = $props();

  type Row = McpToolSummary & { program: string; exposed: boolean };

  let programs = $state<McpProgramTools[] | null>(null);
  let error = $state<string | null>(null);
  let query = $state('');
  let tones = $state<string[]>([]);
  let selected = $state<string | null>(null);
  /** Per-program fold state, so collapsing one survives every keystroke of the filter. */
  let openSections = $state<Record<string, boolean>>({});

  // Reading an inventory starts the backend that holds it, so this is a real wait the
  // first time — hence a loading state rather than an empty list that fills in later.
  $effect(() => {
    void getMcpTools()
      .then((list) => { programs = list; })
      .catch((e) => { error = String(e); programs = []; });
  });

  const SAFETY = {
    read: { label: 'Read', tone: 'info' as const, blurb: 'Observes without changing anything.' },
    write: { label: 'Modify', tone: 'warning' as const, blurb: 'Changes something you can undo.' },
    destructive: {
      label: 'Destructive',
      tone: 'error' as const,
      blurb: 'Deletes, rewrites in bulk, or runs code.',
    },
  };

  const needle = $derived(query.trim().toLowerCase());

  /** Every tool, flattened, so filtering and keyboard motion are one list. */
  const all = $derived<Row[]>(
    (programs ?? []).flatMap((p) =>
      p.tools.map((t) => ({ ...t, program: p.program, exposed: p.exposed })),
    ),
  );

  const rows = $derived(
    all.filter(
      (t) =>
        (tones.length === 0 || tones.includes(t.safety)) &&
        (!needle ||
          t.name.includes(needle) ||
          t.title.toLowerCase().includes(needle) ||
          t.description.toLowerCase().includes(needle)),
    ),
  );

  /** The visible rows regrouped, keeping the backend's program order. */
  const grouped = $derived(
    (programs ?? [])
      .map((p) => ({ ...p, matches: rows.filter((t) => t.program === p.program) }))
      .filter((p) => p.matches.length > 0 || (!needle && tones.length === 0)),
  );

  /**
   * What the detail pane shows: the selected tool while it is still in the list, and the
   * first match otherwise.
   *
   * Resolved here rather than by an effect that corrects `selected` when the filter moves
   * under it. Such an effect reads and writes the same state, which is the shape that
   * loops; and this way a selection that scrolls out of a narrowed list comes back when
   * the list widens again, instead of having been overwritten on the way.
   */
  const current = $derived(rows.find((t) => t.name === selected) ?? rows[0] ?? null);

  const chips = $derived<ChipItem[]>(
    (['read', 'write', 'destructive'] as const).map((key) => ({
      id: key,
      label: SAFETY[key].label,
      count: all.filter((t) => t.safety === key).length,
      tone: SAFETY[key].tone,
      tooltip: SAFETY[key].blurb,
    })),
  );

  /** The product's own name, falling back to the program id for one we have no label for. */
  function label(program: string): string {
    return MCP_PRODUCTS.find((p) => p.id === program)?.name ?? program;
  }

  /**
   * Arrows move the selection while the caret stays in the filter, so narrowing and
   * choosing are one gesture and neither needs the mouse.
   */
  function onKeyDown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    if (rows.length === 0) return;
    e.preventDefault();
    const at = rows.findIndex((t) => t.name === current?.name);
    const next = e.key === 'ArrowDown'
      ? Math.min(at + 1, rows.length - 1)
      : Math.max(at - 1, 0);
    selected = rows[next].name;
    document
      .querySelector(`[data-tool="${CSS.escape(rows[next].name)}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  }
</script>

<Modal {onClose} size="lg" width="880px" height="620px" padBody={false}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Bot size={14} />
      <span class="modal-title">AI tools</span>
      {#if programs}
        <Badge
          variant="pill"
          size="sm"
          label={rows.length === all.length ? `${all.length}` : `${rows.length} of ${all.length}`}
        />
      {/if}
    </ModalHeader>
  {/snippet}

  <div class="tools">
    <div class="toolbar">
      <div class="search">
        <SearchBar
          bind:query
          showRegex={false}
          showCounter={false}
          autofocus
          onkeydown={onKeyDown}
          placeholder="Filter by name, title or what it does…"
          ariaLabel="Filter AI tools"
        />
      </div>
      <ChipBar
        items={chips}
        selected={tones}
        multi
        size="sm"
        tintInactive
        onSelect={(sel) => (tones = Array.isArray(sel) ? sel : [sel])}
      />
    </div>

    {#if !programs}
      <div class="body">
        <StateBlock tone="loading" label="Reading each backend's inventory…">
          {#snippet spinner()}<Spinner size={18} />{/snippet}
        </StateBlock>
      </div>
    {:else if error}
      <div class="body pad">
        <Alert variant="error" title="Arbor could not read the tool list">{error}</Alert>
      </div>
    {:else}
      <div class="body split">
        <nav class="list" aria-label="Tools">
          {#each grouped as program (program.program)}
            <SidebarSection
              label={label(program.program)}
              badge={program.matches.length}
              expanded={openSections[program.program] ?? true}
              onToggle={() =>
                (openSections[program.program] = !(openSections[program.program] ?? true))}
            >
              {#snippet actions()}
                {#if !program.exposed}
                  <Badge variant="tone" tone="neutral" size="sm" label="Off" />
                {/if}
              {/snippet}

              {#if program.matches.length === 0}
                <EmptyState message={program.detail ?? 'No tools.'} compact />
              {:else}
                {#each program.matches as tool (tool.name)}
                  <div data-tool={tool.name}>
                    <SidebarItem
                      selected={current?.name === tool.name}
                      onclick={() => (selected = tool.name)}
                    >
                      {#snippet icon()}
                        <span class="dot" data-safety={tool.safety}></span>
                      {/snippet}
                      <span class="row-name">{tool.name}</span>
                    </SidebarItem>
                  </div>
                {/each}
              {/if}
            </SidebarSection>
          {/each}

          {#if grouped.length === 0}
            <EmptyState
              message="Nothing matches."
              description="The filter looks at the name, the title and the description."
            />
          {/if}
        </nav>

        <article class="detail">
          {#if current}
            <header class="detail-head">
              <h2>{current.title}</h2>
              <div class="ident">
                <code>{current.name}</code>
                <CopyButton value={current.name} title="Copy the tool name" />
              </div>
              <div class="pills">
                <Badge
                  variant="tone"
                  tone={SAFETY[current.safety]?.tone ?? 'neutral'}
                  size="sm"
                  label={SAFETY[current.safety]?.label ?? current.safety}
                />
                <span class="pill-note">{SAFETY[current.safety]?.blurb ?? ''}</span>
              </div>
            </header>

            {#if !current.exposed}
              <Alert variant="warning" compact>
                {label(current.program)} is not exposed right now, so a client cannot call
                this. Turn it on under Settings → AI tool access.
              </Alert>
            {/if}

            <p class="description">{current.description}</p>

            <dl class="facts">
              <dt>Handler</dt>
              <dd><code>{current.method}</code></dd>
              <dt>Repeatable</dt>
              <dd>
                {current.idempotent
                  ? 'Calling it twice is the same as calling it once.'
                  : 'Each call does the work again.'}
              </dd>
            </dl>
          {:else}
            <StateBlock tone="neutral" label="Pick a tool to see what it does." />
          {/if}
        </article>
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="hint">
        This is what a connected client can see. What it may actually run is decided by your
        permissions, under Settings → AI tool access.
      </span>
      <Button variant="primary" onclick={onClose}>Close</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .tools {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .search { flex: 1; min-width: 0; }

  .body {
    flex: 1;
    min-height: 0;
  }
  .body.pad { padding: 12px; }

  /* The two columns of the reference: what is there, and what this one does. */
  .split {
    display: grid;
    grid-template-columns: 268px 1fr;
  }

  .list {
    overflow-y: auto;
    border-right: 1px solid var(--border);
    padding-bottom: 8px;
  }

  .row-name {
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* The class of action, carried by colour alone in the list — the word for it is one
     glance away in the detail pane, and a badge per row would crowd out the name. */
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    display: block;
    margin-left: 4px;
  }
  .dot[data-safety='read'] { background: var(--info); }
  .dot[data-safety='write'] { background: var(--warning); }
  .dot[data-safety='destructive'] { background: var(--error); }

  .detail {
    overflow-y: auto;
    padding: 16px 20px 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .detail-head {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .detail-head h2 {
    margin: 0;
    font-size: var(--font-size-md);
    font-weight: 600;
    color: var(--text-primary);
  }

  .ident {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .ident code {
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    color: var(--accent);
  }

  .pills {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .pill-note {
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
  }

  /* The prose is the point of this pane: give it a measure that reads. */
  .description {
    margin: 0;
    max-width: 60ch;
    font-size: var(--font-size-sm);
    line-height: 1.65;
    color: var(--text-secondary);
    white-space: pre-line;
  }

  .facts {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
    margin: 0;
    padding-top: 8px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs);
  }
  .facts dt { color: var(--text-disabled); }
  .facts dd { margin: 0; color: var(--text-secondary); }
  .facts code { font-family: var(--font-mono); }

  .hint {
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
    max-width: 60ch;
  }
</style>
