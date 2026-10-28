<!--
  One live grant.

  A grant is a permission that is running out while you look at it, so the card leads with
  what is left of it — as a fraction of what it started with, because "3 left" says nothing
  without "of 50" — and only then with what it covers. The products it switches on are named
  first among those: that is the part a reader cannot infer from the settings elsewhere on
  this page, which still say the product is off.
-->
<script lang="ts">
  import { tooltip } from '$lib/actions/tooltip';
  import { Trash2 } from 'lucide-svelte';

  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { MCP_PRODUCTS } from '$lib/types/mcp';
  import type { McpGrant } from '$lib/types/mcp';

  let { grant, onrevoke }: { grant: McpGrant; onrevoke: () => void } = $props();

  const productName = (id: string) => MCP_PRODUCTS.find((p) => p.id === id)?.name ?? id;

  /** The shorter of the two budgets is the one that will actually end the grant. */
  const usesLeft = $derived(
    grant.uses_left !== null && grant.max_uses
      ? { left: grant.uses_left, of: grant.max_uses, pct: (grant.uses_left / grant.max_uses) * 100 }
      : null,
  );
  const timeLeft = $derived(
    grant.expires_in_secs !== null && grant.ttl_secs
      ? {
          label: humanTime(grant.expires_in_secs),
          pct: (grant.expires_in_secs / grant.ttl_secs) * 100,
        }
      : null,
  );

  function humanTime(secs: number): string {
    if (secs < 60) return `${Math.max(1, Math.round(secs))}s`;
    const mins = Math.round(secs / 60);
    if (mins < 120) return `${mins} min`;
    return `${Math.round(mins / 60)} h`;
  }

  /** Anything under a fifth of the budget is about to go, and is worth saying in colour. */
  const runningOut = $derived((usesLeft?.pct ?? 100) < 20 || (timeLeft?.pct ?? 100) < 20);
</script>

<div class="card" class:low={runningOut}>
  <div class="head">
    <strong>{grant.label}</strong>
    <span class="spacer"></span>
    {#each grant.programs as program (program)}
      <Badge variant="tone" tone="accent" size="sm">{productName(program)}</Badge>
    {/each}
    <Button
      variant="ghost"
      size="sm"
      ariaLabel="Revoke the grant {grant.label}"
      onclick={onrevoke}>
      {#snippet iconStart()}<Trash2 size={14} />{/snippet}
    </Button>
  </div>

  <div class="budget">
    {#if usesLeft}
      <div class="meter" use:tooltip={`${usesLeft.left} of ${usesLeft.of} calls left`}>
        <span class="meter-label">{usesLeft.left} of {usesLeft.of} calls</span>
        <div class="track"><div class="fill" style:width="{Math.min(100, usesLeft.pct)}%"></div></div>
      </div>
    {/if}
    {#if timeLeft}
      <div class="meter" use:tooltip={`${timeLeft.label} left`}>
        <span class="meter-label">{timeLeft.label} left</span>
        <div class="track"><div class="fill" style:width="{Math.min(100, timeLeft.pct)}%"></div></div>
      </div>
    {/if}
    {#if !usesLeft && !timeLeft}
      <span class="meter-label">no limit reported</span>
    {/if}
  </div>

  <dl class="covers">
    <dt>Projects</dt>
    <dd>
      {#each grant.roots as root (root)}
        <code>{root}</code>
      {/each}
    </dd>
    <dt>Tools</dt>
    <dd class="tools">{grant.tools.join(', ')}</dd>
  </dl>
</div>

<style>
  .card    { display: flex; flex-direction: column; gap: 8px; padding: 9px 10px;
             background: var(--bg-base); border: 1px solid var(--border-subtle);
             border-radius: var(--radius-md); }
  /* A grant close to running out is the one you may want to renew before you walk away. */
  .card.low { border-color: var(--warning-subtle); }

  .head    { display: flex; align-items: center; gap: 6px; }
  .head strong { font-size: 12.5px; color: var(--text-primary); }
  .spacer  { flex: 1; }

  .budget  { display: flex; gap: 14px; }
  .meter   { display: flex; flex-direction: column; gap: 3px; flex: 1; min-width: 0; }
  .meter-label { font-size: 11px; color: var(--text-tertiary); }
  .track   { height: 3px; border-radius: 2px; background: var(--bg-elevated);
             overflow: hidden; }
  .fill    { height: 100%; background: var(--accent); border-radius: 2px; }
  .card.low .fill { background: var(--warning); }

  .covers  { display: grid; grid-template-columns: auto 1fr; gap: 2px 10px; margin: 0; }
  dt       { font-size: 11px; color: var(--text-tertiary); }
  dd       { margin: 0; min-width: 0; display: flex; flex-wrap: wrap; gap: 4px; }
  dd code  { font-family: var(--font-mono); font-size: 11px; color: var(--text-secondary);
             overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .tools   { font-size: 11px; color: var(--text-secondary); }
</style>
