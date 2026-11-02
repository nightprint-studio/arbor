<script lang="ts">
  /**
   * The JNDI names the project itself asks for.
   *
   * Bennu has already read `web.xml`, `@Resource`, `lookup()`, `persistence.xml` and the Spring XML,
   * so it knows what the application expects to find in its context. Listing those — each with the
   * line it came from, so a surprising one can be checked — turns "write a context.xml" into "fill
   * in the ones that are missing", and a name that is missing is the difference between an
   * application that starts and a `NameNotFoundException` after it has.
   */
  import { Check, Plus, TriangleAlert } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import type { TomcatRequirement } from '$lib/ipc/bennu/tomcat';

  let {
    requirements,
    defined,
    onadd,
    onclose,
  }: {
    requirements: TomcatRequirement[];
    /** Names the context already defines. */
    defined: Set<string>;
    onadd: (names: string[]) => void;
    onclose: () => void;
  } = $props();

  const missing = $derived(requirements.filter((r) => !defined.has(r.name)));
</script>

<div class="head">
  <strong>Required by the project</strong>
  <span class="sp"></span>
  <Button variant="primary" size="sm" disabled={missing.length === 0} onclick={() => onadd(missing.map((m) => m.name))}>
    Add the {missing.length} missing
  </Button>
  <Button variant="secondary" size="sm" onclick={onclose}>Close</Button>
</div>
<p class="lead">
  Read from <code>web.xml</code>, <code>@Resource</code>, <code>lookup()</code>, <code>persistence.xml</code> and Spring XML.
  Adding creates the resource; its URL and credentials are still yours to fill in.
</p>

{#if requirements.length === 0}
  <EmptyState message="The project does not declare any JNDI resource." compact />
{:else}
  <ul class="list">
    {#each requirements as r (r.name)}
      {@const have = defined.has(r.name)}
      <li class="item">
        <span class="mark" class:ok={have} class:miss={!have}>
          {#if have}<Check size={14} />{:else}<TriangleAlert size={14} />{/if}
        </span>
        <span class="tx">
          <span class="name">{r.name}</span>
          <span class="src">{r.kind === 'env' ? 'environment entry' : 'data source'} · {r.source}</span>
        </span>
        {#if have}
          <span class="have">defined</span>
        {:else}
          <Button variant="secondary" size="xs" onclick={() => onadd([r.name])}>
            {#snippet iconStart()}<Plus size={12} />{/snippet}
            Add
          </Button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .head { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
  .sp { flex: 1; }
  .lead { margin: 0 0 12px; color: var(--text-muted); font-size: var(--font-size-xs); line-height: 1.5; }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .item { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-base); }
  .tx { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .name { font: 600 var(--font-size-xs) var(--font-code); color: var(--text-primary); }
  .src { font-size: var(--font-size-2xs, 11px); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mark { display: inline-flex; }
  .mark.ok { color: var(--success); }
  .mark.miss { color: var(--warning); }
  .have { color: var(--text-muted); font-size: var(--font-size-xs); }
  code { font-family: var(--font-code); }
</style>
