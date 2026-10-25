<!--
  The tool picker of a grant: every tool the AI surface can carry, grouped by the product
  that serves it, with what the grant would switch on said on the group's own row.

  Read tools are offered too, which they were not while a grant only stood in for a consent
  prompt: a read never prompts, so there was nothing to pre-approve. Now that a grant
  overrides the gates, a product that is off refuses its reads as well — so what a grant has
  to be able to cover is `corvus_status`, not only `corvus_commit`.
-->
<script lang="ts">
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { MCP_PRODUCTS } from '$lib/types/mcp';
  import type { McpProgramTools } from '$lib/types/mcp';

  let {
    programs,
    chosen,
    onchange,
  }: {
    programs: McpProgramTools[];
    chosen: Record<string, boolean>;
    onchange: (next: Record<string, boolean>) => void;
  } = $props();

  const withTools = $derived(programs.filter((p) => p.tools.length > 0));

  const productName = (id: string) => MCP_PRODUCTS.find((p) => p.id === id)?.name ?? id;

  // Annotated rather than inferred: `Badge`'s `tone` is a union, and a plain `string` here
  // is the kind of thing svelte-check rejects only once, at the call site, where it reads
  // like the badge's fault.
  const safetyTone = (safety: string): 'error' | 'warning' | 'neutral' =>
    safety === 'destructive' ? 'error' : safety === 'write' ? 'warning' : 'neutral';

  const pickedIn = (program: McpProgramTools) =>
    program.tools.filter((t) => chosen[t.name]).length;

  function setMany(names: string[], value: boolean) {
    const next = { ...chosen };
    for (const name of names) {
      if (value) next[name] = true;
      else delete next[name];
    }
    onchange(next);
  }

  function setOne(name: string, value: boolean) {
    setMany([name], value);
  }
</script>

<div class="picker">
  {#each withTools as program (program.program)}
    {@const picked = pickedIn(program)}
    <div class="group">
      <div class="group-head">
        <span class="group-name">{productName(program.program)}</span>
        {#if program.exposed}
          <Badge variant="tone" tone="success" size="sm">on the surface</Badge>
        {:else}
          <Badge variant="tone" tone="info" size="sm">off — the grant switches it on</Badge>
        {/if}
        <span class="spacer"></span>
        {#if picked > 0}
          <span class="picked">{picked} of {program.tools.length}</span>
        {/if}
        <Button
          variant="ghost"
          size="sm"
          onclick={() => setMany(program.tools.map((t) => t.name), picked < program.tools.length)}>
          {picked < program.tools.length ? 'All' : 'None'}
        </Button>
      </div>

      <div class="tools">
        {#each program.tools as tool (tool.name)}
          <div class="tool">
            <Toggle
              checked={chosen[tool.name] ?? false}
              label={tool.title}
              description={tool.name}
              onchange={(v: boolean) => setOne(tool.name, v)} />
            <Badge variant="tone" tone={safetyTone(tool.safety)} size="sm">{tool.safety}</Badge>
          </div>
        {/each}
      </div>
    </div>
  {:else}
    <EmptyState
      compact
      message="No tools to grant"
      description="No backend answered with an inventory. Arbor starts each one to read it, so this usually means a product failed to launch." />
  {/each}
</div>

<style>
  .picker     { display: flex; flex-direction: column; gap: 12px; }

  .group      { display: flex; flex-direction: column; gap: 6px;
                background: var(--bg-base); border: 1px solid var(--border-subtle);
                border-radius: var(--radius-md); padding: 8px 10px; }
  .group-head { display: flex; align-items: center; gap: 8px; }
  .group-name { font-size: 12.5px; font-weight: 500; color: var(--text-primary); }
  .spacer     { flex: 1; }
  .picked     { font-size: 11px; color: var(--text-tertiary); }

  .tools      { display: flex; flex-direction: column; gap: 6px; }
  /* The safety badge sits on the row's trailing edge, so a column of them reads as a
     column: it is the thing you scan when deciding what a grant is worth. */
  .tool       { display: flex; align-items: center; gap: 10px; }
  .tool :global(> *:first-child) { flex: 1; min-width: 0; }
</style>
