<!--
  What an AI client is doing, and what it did.

  The panel the whole permission model owes the user. Settings answer "what could happen";
  only this answers "what is happening" and "what happened" — and a permission model that
  cannot be audited after the fact is one you have to trust prospectively, which is exactly
  the posture it exists to avoid.

  One implementation, framed twice: as a page of the AI settings, and as a window of its
  own reachable from anywhere (`McpActivityModal`). The endpoint is process-wide and so is
  the question, so a second copy would be a second place for the two to disagree.

  Four deliberate choices:

  1. **A row appears when the call arrives, not when it ends.** A log that only shows
     finished calls is blank for exactly as long as something is running, which is the only
     time anyone is watching. A row live-updates through waiting → asking → running → done.
  2. **A running call shows what it is saying about itself** — the same lines an AI client
     is sent as progress. "It is running the tests" and "it is on OrderTest, 12 passed" are
     different amounts of knowing.
  3. **Refusals are rows too**, not silence. A refused call is the interesting one: either
     the gates working, or a setting tighter than you meant. Both need seeing.
  4. **Arguments are shown**, truncated by the backend rather than hidden — but on one
     line until asked for. "read a file" and "read *that* file" are different events and
     only the second is auditable, so they are never hidden outright; a screenful of JSON
     per row is a page nobody reads, and the identifying part is at the front of the line.
  5. **The product is a filter, not a column.** Every call names the backend that served
     it, but the tool's own name almost always starts with it — so the product picker is in
     the bar and the label on a row appears only when the name does not already carry it.
  6. **Earlier runs are kept**, and marked as such. The row you want to look at is often
     from the session where you noticed something odd, not from this one — but a row from
     an hour ago and one from last week should not read the same, so an inherited row says
     so and carries its date.
-->
<script lang="ts">
  import { ChevronDown, ChevronRight, RefreshCw, Trash2 } from 'lucide-svelte';

  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ChipBar from '$lib/components/shared/ui/ChipBar.svelte';
  import type { ChipItem } from '$lib/components/shared/ui/ChipBar.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { mcpStore } from '$lib/stores/mcp.svelte';
  import { MCP_PRODUCTS } from '$lib/types/mcp';
  import type { McpAuditEntry } from '$lib/types/mcp';

  let {
    /** Wraps a store write so the frame can report a failure its own way. */
    guard = (action: () => Promise<void>) => action(),
    /**
     * Claim the frame's height and scroll the rows inside it, instead of growing with them.
     *
     * A prop because only the frame knows: in a window the filter bar must stay put while
     * a long log scrolls under it, and on a settings page that is one scrollbar inside
     * another. CSS cannot tell which it is in.
     */
    fill = false,
  }: {
    guard?: (action: () => Promise<void>) => Promise<void>;
    fill?: boolean;
  } = $props();

  // The live mirror fills from `arbor://mcp-call` in every window, but only from the
  // moment that window existed. The backend's ring is the record, so the panel asks.
  $effect(() => { void mcpStore.refreshAudit().catch(() => {}); });

  const entries = $derived(mcpStore.audit);
  const run = $derived(mcpStore.auditRun);

  let thisRunOnly = $state(false);
  let program = $state('all');

  /**
   * The programs that appear in the log, not the ones that could.
   *
   * Built from the entries so the picker never offers a product nothing has called — and
   * so it keeps working for a program that is not in `MCP_PRODUCTS` at all, which is where
   * a name would otherwise be silently dropped.
   */
  const programs = $derived(
    [...new Set(entries.map((e) => e.program))].sort().map((id) => ({
      value: id,
      label: MCP_PRODUCTS.find((p) => p.id === id)?.name ?? id,
    })),
  );

  const programOptions = $derived([{ value: 'all', label: 'All products' }, ...programs]);

  // Same shape as the bucket fallback: a product can leave the log when it is cleared, and
  // correcting `program` in an effect would be a read and a write of one piece of state.
  const activeProgram = $derived(
    programOptions.some((o) => o.value === program) ? program : 'all',
  );

  const inherited = $derived(entries.filter((e) => e.run !== run).length);

  /** What each state is called, how loudly, and whether it is still going. */
  const OUTCOME: Record<
    McpAuditEntry['outcome'],
    { label: string; tone: 'success' | 'warning' | 'error' | 'neutral' | 'info'; live?: boolean }
  > = {
    waiting:       { label: 'Starting',         tone: 'info',    live: true },
    asking:        { label: 'Waiting for you',  tone: 'warning', live: true },
    running:       { label: 'Running',          tone: 'info',    live: true },
    allowed:       { label: 'Ran',              tone: 'success' },
    asked_allowed: { label: 'You approved',     tone: 'success' },
    asked_denied:  { label: 'You refused',      tone: 'warning' },
    denied:        { label: 'Refused by rules', tone: 'error' },
    timed_out:     { label: 'Prompt timed out', tone: 'warning' },
    failed:        { label: 'Failed',           tone: 'neutral' },
    interrupted:   { label: 'Interrupted',      tone: 'neutral' },
  };

  /**
   * The buckets, in the order a reader wants them.
   *
   * "Ran freely" and "you approved" are split because they answer different questions: the
   * first is what your settings let through without asking, which is the set worth
   * reviewing; the second is what you personally let through, which you already know about.
   * There is no combined "asked" bucket — with approved and declined both present it says
   * nothing neither of them does.
   */
  const BUCKETS: {
    id: string;
    label: string;
    tone: ChipItem['tone'];
    match: (e: McpAuditEntry) => boolean;
  }[] = [
    { id: 'live', label: 'In flight', tone: 'info',
      match: (e) => !!OUTCOME[e.outcome]?.live },
    { id: 'free', label: 'Ran freely', tone: 'success',
      match: (e) => e.outcome === 'allowed' },
    { id: 'approved', label: 'You approved', tone: 'success',
      match: (e) => e.outcome === 'asked_allowed' },
    { id: 'declined', label: 'You declined', tone: 'warning',
      match: (e) => e.outcome === 'asked_denied' || e.outcome === 'timed_out' },
    { id: 'refused', label: 'Refused by rules', tone: 'error',
      match: (e) => e.outcome === 'denied' },
    { id: 'failed', label: 'Failed', tone: 'neutral',
      match: (e) => e.outcome === 'failed' || e.outcome === 'interrupted' },
  ];

  let filter = $state<string>('all');

  // Only the buckets that have something in them. A strip of six chips reading zero is a
  // strip you stop reading; the ones that appear are the ones worth a click.
  const chips = $derived<ChipItem[]>([
    { id: 'all', label: 'All', count: entries.length, tone: 'muted' },
    ...BUCKETS.map((b) => ({
      id: b.id,
      label: b.label,
      count: entries.filter(b.match).length,
      tone: b.tone,
    })).filter((c) => c.count > 0),
  ]);

  // A bucket can empty while it is selected — a run finishes and "In flight" goes to zero.
  // Falling back in the derived, rather than correcting `filter` in an effect, keeps this
  // from being a read and a write of the same state.
  const active = $derived(chips.some((c) => c.id === filter) ? filter : 'all');

  const shown = $derived(
    (active === 'all'
      ? entries
      : entries.filter(BUCKETS.find((b) => b.id === active)?.match ?? (() => true))
    )
      .filter((e) => !thisRunOnly || e.run === run)
      .filter((e) => activeProgram === 'all' || e.program === activeProgram),
  );

  /** Rows whose progress the reader has opened. Running ones start open. */
  let expanded = $state<Record<string, boolean>>({});
  /** Rows whose arguments the reader has opened. */
  let argsOpen = $state<Record<string, boolean>>({});

  /** `(run, id)`, because the backend's id counter restarts with the process. */
  function key(entry: McpAuditEntry): string {
    return `${entry.run}:${entry.id}`;
  }

  function isOpen(entry: McpAuditEntry): boolean {
    return expanded[key(entry)] ?? !!OUTCOME[entry.outcome]?.live;
  }

  /**
   * The arguments, laid out, when someone asks for them.
   *
   * The backend sends compact JSON and truncates a long one, so a truncated payload will
   * not parse — falling back to the raw text is the honest answer there rather than an
   * error where the arguments should be.
   */
  function laidOut(text: string): string {
    try {
      return JSON.stringify(JSON.parse(text), null, 2);
    } catch {
      return text;
    }
  }

  /**
   * The clock for this run, the date for anything older.
   *
   * A time alone reads as "a moment ago" whatever day it is from, which is the wrong thing
   * for a log that now outlives the session that wrote it.
   */
  function when(entry: McpAuditEntry): string {
    const d = new Date(entry.at);
    const clock = d.toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
    return entry.run === run ? clock : `${d.toLocaleDateString()} ${clock}`;
  }
</script>

<div class="activity" class:fill>
  <div class="bar">
    <ChipBar
      items={chips}
      selected={active}
      size="sm"
      tintInactive
      onSelect={(sel) => (filter = Array.isArray(sel) ? (sel[0] ?? 'all') : sel)} />
    <span class="spacer"></span>
    {#if programs.length > 1}
      <Select
        size="sm"
        value={activeProgram}
        options={programOptions}
        highlight={activeProgram !== 'all'}
        ariaLabel="Filter by product"
        onchange={(v: string) => (program = v)} />
    {/if}
    {#if inherited > 0}
      <Button
        size="sm"
        variant={thisRunOnly ? 'secondary' : 'ghost'}
        title={`${inherited} rows were carried over from earlier runs`}
        onclick={() => (thisRunOnly = !thisRunOnly)}>
        This run only
      </Button>
    {/if}
    <Button
      variant="icon"
      ariaLabel="Refresh"
      title="Re-read the log from the backend"
      onclick={() => guard(() => mcpStore.refreshAudit())}>
      <RefreshCw size={14} />
    </Button>
    <Button
      variant="icon"
      ariaLabel="Clear the log"
      title="Forget every row — it holds the paths an assistant has looked at"
      disabled={entries.length === 0}
      onclick={() => guard(() => mcpStore.clearAudit())}>
      <Trash2 size={14} />
    </Button>
  </div>

  <div class="rows">
    {#each shown as entry (`${entry.run}:${entry.id}`)}
      {@const state = OUTCOME[entry.outcome] ?? OUTCOME.failed}
      <div class="row" class:live={state.live}>
        <div class="head">
          <span class="time">{when(entry)}</span>
          <code class="tool">{entry.tool}</code>
          {#if !entry.tool.startsWith(`${entry.program}_`)}
            <!-- Tool names are usually prefixed with their product, and repeating it
                 ("bennu · bennu_test_run") is noise. Shown when the prefix is absent —
                 which `mcp(name = …)` is free to make the case. -->
            <span class="program">{entry.program}</span>
          {/if}
          {#if entry.run !== run}
            <span class="earlier">earlier run</span>
          {/if}
          <span class="spacer"></span>
          {#if entry.duration_ms !== null}
            <span class="ms">{entry.duration_ms} ms</span>
          {/if}
          {#if state.live}<Spinner size={11} />{/if}
          <Badge variant="tone" tone={state.tone} size="sm" label={state.label} />
        </div>

        <!-- One line by default. The arguments are the auditable part, so they are never
             hidden outright — but a screenful of JSON per row is a page nobody reads, and
             the identifying part (the path, the first selector) is at the front anyway. -->
        <button
          type="button"
          class="args"
          class:open={argsOpen[key(entry)]}
          title={argsOpen[key(entry)] ? 'Collapse the arguments' : 'Show the whole call'}
          onclick={() => (argsOpen[key(entry)] = !argsOpen[key(entry)])}>
          {argsOpen[key(entry)] ? laidOut(entry.arguments) : entry.arguments}
        </button>

        {#if entry.progress.length > 0}
          <button
            type="button"
            class="disclose"
            onclick={() => (expanded[key(entry)] = !isOpen(entry))}>
            {#if isOpen(entry)}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
            {entry.progress.length} line{entry.progress.length === 1 ? '' : 's'} of output
          </button>
          {#if isOpen(entry)}
            <!-- Newest last, like a console: what it is doing NOW is the bottom line, and
                 a reader watching a live run watches the bottom. -->
            <pre class="output">{entry.progress.join('\n')}</pre>
          {/if}
        {/if}

        {#if entry.detail}
          <!-- For a refusal this is the reason the model was given — the same words it
               read, so what it did next stops being a mystery. -->
          <p class="detail">{entry.detail}</p>
        {/if}
      </div>
    {:else}
      <EmptyState
        compact
        message={entries.length === 0 ? 'Nothing has been called yet' : 'No call matches this filter'}
        description={entries.length === 0
          ? 'Calls appear here the moment they arrive, whether they run or are refused. The log survives a restart — Clear removes it.'
          : undefined} />
    {/each}
  </div>
</div>

<style>
  .activity { display: flex; flex-direction: column; gap: 10px; min-height: 0; }
  .activity.fill { height: 100%; }

  .bar     { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; flex: none; }
  .spacer  { flex: 1; }

  .rows    { display: flex; flex-direction: column; gap: 6px; min-height: 0; }
  .activity.fill .rows { flex: 1; overflow-y: auto; padding-right: 2px; }
  .row     { display: flex; flex-direction: column; gap: 5px; padding: 8px 10px;
             background: var(--bg-base); border: 1px solid var(--border-subtle);
             border-radius: var(--radius-md); }
  /* Still going: the one thing on this page you might want to act on. */
  .row.live { border-color: color-mix(in srgb, var(--info) 45%, var(--border-subtle)); }

  .head    { display: flex; align-items: center; gap: 8px; }
  .time    { font-family: var(--font-mono); font-size: 11px; color: var(--text-tertiary);
             font-variant-numeric: tabular-nums; }
  .tool    { font-family: var(--font-mono); font-size: 12px; font-weight: 500;
             color: var(--text-primary); }
  /* Quiet: it qualifies the row rather than competing with what the row says. */
  .earlier { font-size: 10.5px; color: var(--text-disabled); }
  .program { font-size: 10.5px; color: var(--text-tertiary); text-transform: uppercase;
             letter-spacing: 0.04em; }
  .ms      { font-size: 11px; color: var(--text-tertiary); font-variant-numeric: tabular-nums; }

  /* Collapsed: one line that ends in an ellipsis. Open: the call, laid out. */
  .args    { display: block; width: 100%; margin: 0; padding: 3px 6px; text-align: left;
             background: none; border: none; border-radius: var(--radius-sm);
             cursor: pointer; font-family: var(--font-mono); font-size: 11px;
             line-height: 1.45; color: var(--text-tertiary);
             white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .args:hover { background: var(--bg-hover); color: var(--text-secondary); }
  .args.open { max-height: 220px; overflow: auto; white-space: pre; cursor: zoom-out;
             color: var(--text-secondary); background: var(--bg-elevated); }

  .disclose { display: inline-flex; align-items: center; gap: 4px; align-self: flex-start;
             padding: 2px 4px; background: none; border: none; cursor: pointer;
             font-size: 11px; color: var(--text-tertiary); border-radius: var(--radius-sm); }
  .disclose:hover { background: var(--bg-hover); color: var(--text-secondary); }

  .output  { margin: 0; max-height: 160px; overflow: auto; padding: 6px 8px;
             background: var(--bg-elevated); border-radius: var(--radius-sm);
             font-family: var(--font-mono); font-size: 11px; line-height: 1.5;
             color: var(--text-secondary); white-space: pre-wrap; word-break: break-word; }

  .detail  { margin: 0; font-size: 11.5px; line-height: 1.45; color: var(--text-tertiary); }
</style>
