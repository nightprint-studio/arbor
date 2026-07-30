<script lang="ts">
  /**
   * A query plan, drawn.
   *
   * ## The one thing this screen must never get wrong
   *
   * A plan is either a **description** of what the server intends or a **record**
   * of what it did, and the numbers look identical. Showing an estimate where a
   * reader expects a measurement is how "the query returns 12 rows" becomes a
   * production incident — so the distinction is not a footnote here: it is the
   * first band of the panel, in words, and every measured column is labelled
   * "actual" wherever it appears.
   *
   * ## Indentation is the tree
   *
   * The backend flattens the plan to a depth-tagged list in execution-tree order,
   * so this renders an indented list and no recursion is needed. A child feeds the
   * node above it, which is the direction the eye reads a plan in.
   */
  import { Calculator, Gauge, FileText } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { formatElapsed } from '$lib/stores/picus/query.svelte';
  import type { PlanNode, QueryPlan } from '$lib/ipc/picus/plan';

  interface Props {
    plan: QueryPlan;
    /** The statement this plan is about, shown so a stale plan is recognisable. */
    sql?: string;
  }

  let { plan, sql = '' }: Props = $props();

  /** The engine's own text, for the moment somebody wants to paste it somewhere. */
  let showText = $state(false);

  /**
   * How far the estimate missed, as a signed factor — positive when the planner
   * expected too few rows.
   *
   * Clamped to one on both sides: a zero is a division, and "the planner expected
   * none and got none" is not a discrepancy.
   */
  function deviation(node: PlanNode): number | null {
    if (!plan.analyzed || node.rows === null || node.actualRows === null) return null;
    const expected = Math.max(node.rows, 1);
    const got = Math.max(node.actualRows, 1);
    return got >= expected ? got / expected : -(expected / got);
  }

  /**
   * Only a real discrepancy is marked.
   *
   * Under a factor of two the planner was right; marking that would put a badge on
   * every node of every plan, which is the same as marking nothing.
   */
  const MARK_FROM = 2;
  /** Where a discrepancy stops being interesting and starts being the answer. */
  const SERIOUS_FROM = 10;

  function rows(n: number | null): string {
    return n === null ? '—' : Math.round(n).toLocaleString();
  }

  function cost(n: number | null): string {
    return n === null ? '—' : n.toFixed(2);
  }
</script>

<div class="pl">
  <!-- Estimate or measurement, said in words before any number is read. -->
  <div class="pl-kind" class:pl-measured={plan.analyzed}>
    {#if plan.analyzed}<Gauge size={14} />{:else}<Calculator size={14} />{/if}
    <strong>{plan.analyzed ? 'Measured' : 'Estimate'}</strong>
    <span class="pl-kind-note">
      {plan.analyzed
        ? 'The statement was executed. Row counts and times below are what actually happened.'
        : 'Nothing was executed. Every number below is the planner’s prediction.'}
    </span>
    <span class="pl-spacer"></span>
    {#if plan.totalCost !== null}
      <span class="pl-total" use:tooltip={'Total estimated cost of the root node, in the planner’s own units — comparable between plans of the same statement, meaningless on its own.'}>
        cost {cost(plan.totalCost)}
      </span>
    {/if}
    {#if plan.analyzed && plan.actualMs !== null}
      <span class="pl-total pl-total-actual">actual {formatElapsed(plan.actualMs)}</span>
    {/if}
    <Button
      variant="icon"
      size="xs"
      tooltip={showText ? 'Back to the plan' : 'The engine’s own text — what you paste into a ticket'}
      ariaLabel="Toggle the engine's own plan text"
      onclick={() => (showText = !showText)}
    >
      {#snippet iconStart()}<FileText size={13} />{/snippet}
    </Button>
    <CopyButton
      value={plan.text}
      title="Copy the plan as the engine printed it"
      toastSuccess="Plan copied."
    />
  </div>

  {#if sql}
    <!-- Which statement this is about. A plan outlives the caret that asked for
         it, and a plan of the statement above the one on screen is worse than no
         plan, so it says so rather than being inferred from position. -->
    <div class="pl-sql" use:tooltip={sql}><code>{sql}</code></div>
  {/if}

  {#if showText}
    <pre class="pl-text">{plan.text}</pre>
  {:else}
    <ul class="pl-nodes">
      {#each plan.nodes as node, i (i)}
        {@const off = deviation(node)}
        <li class="pl-node" style="padding-left: {8 + node.depth * 16}px">
          <div class="pl-row">
            {#if node.depth > 0}<span class="pl-arm" aria-hidden="true">└</span>{/if}
            <span class="pl-label">{node.label}</span>
            {#if node.relation}<span class="pl-rel">on {node.relation}</span>{/if}
            <span class="pl-spacer"></span>

            <!-- Estimates, always present, always named as estimates. -->
            <span class="pl-num" use:tooltip={'The planner’s estimate — rows out of this node, per loop.'}>
              ~{rows(node.rows)} rows
            </span>
            <span class="pl-num pl-cost" use:tooltip={'Estimated total cost at this node.'}>
              {cost(node.cost)}
            </span>

            {#if plan.analyzed}
              <span class="pl-num pl-actual" use:tooltip={'Rows this node really produced, per loop.'}>
                {rows(node.actualRows)} actual
              </span>
              {#if node.actualMs !== null}
                <span class="pl-num pl-actual">{formatElapsed(node.actualMs)}</span>
              {/if}
              {#if off !== null && Math.abs(off) >= MARK_FROM}
                <Badge
                  variant="tone"
                  size="sm"
                  tone={Math.abs(off) >= SERIOUS_FROM ? 'error' : 'warning'}
                  label={`${off > 0 ? '↑' : '↓'}×${Math.round(Math.abs(off))}`}
                />
              {/if}
            {/if}
          </div>

          {#if node.detail.length}
            <div class="pl-detail">
              {#each node.detail as line, j (j)}<span>{line}</span>{/each}
            </div>
          {/if}

          {#if node.warning}
            <!-- Prose, and inline rather than behind a pointer: it is the part of
                 the screen worth reading, and advice you have to hover to find is
                 advice nobody reads. -->
            <p class="pl-warn">{node.warning}</p>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .pl { display: flex; flex-direction: column; min-height: 0; width: 100%; }
  .pl-spacer { flex: 1; min-width: 8px; }

  /* The band that says which of the two things this is. Deliberately the loudest
     element of the panel — everything under it is numbers that look the same
     either way. */
  .pl-kind {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    padding: 5px 10px 5px 12px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    color: var(--info);
    font-size: var(--font-size-xs);
  }
  .pl-kind strong { text-transform: uppercase; letter-spacing: 0.06em; font-size: var(--font-size-2xs); }
  .pl-measured { color: var(--success); }
  .pl-kind-note { color: var(--text-muted); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .pl-total {
    flex-shrink: 0;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    cursor: help;
  }
  .pl-total-actual { color: var(--success); }

  .pl-sql {
    flex-shrink: 0;
    padding: 4px 12px;
    border-bottom: 1px solid var(--border-subtle);
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pl-sql code { font-family: inherit; }

  .pl-nodes { flex: 1; min-height: 0; overflow: auto; list-style: none; margin: 0; padding: 4px 0; }

  .pl-node { padding-right: 10px; padding-top: 2px; padding-bottom: 2px; }
  .pl-node + .pl-node { border-top: 1px solid color-mix(in srgb, var(--border-subtle) 45%, transparent); }

  .pl-row { display: flex; align-items: center; gap: 8px; min-height: 20px; }
  .pl-arm { color: var(--text-disabled); flex-shrink: 0; }
  .pl-label {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-primary);
    white-space: nowrap;
  }
  .pl-rel { font-size: var(--font-size-2xs); color: var(--accent); white-space: nowrap; }

  .pl-num {
    flex-shrink: 0;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    white-space: nowrap;
  }
  .pl-cost { color: var(--text-disabled); }
  /* Measurements are the other colour on purpose: on an analysed plan the two
     numbers sit side by side and the eye has to be able to tell which is which
     without reading the word. */
  .pl-actual { color: var(--success); }

  .pl-detail {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    padding: 1px 0 2px 14px;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
  }

  .pl-warn {
    margin: 3px 0 4px 14px;
    padding: 5px 8px;
    border-left: 2px solid var(--warning);
    background: color-mix(in srgb, var(--warning) 8%, transparent);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    font-size: var(--font-size-2xs);
    line-height: 1.5;
    color: var(--text-secondary);
  }

  .pl-text {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0;
    padding: 8px 12px;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.5;
    color: var(--text-secondary);
    white-space: pre;
  }
</style>
