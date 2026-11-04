<script lang="ts">
  /**
   * VariablesUsed — the variables this request uses, each with its state: defined (and by which layer, with its
   * value, or masked when it is a secret) or not defined (with a button to define it). It answers "what does
   * this request need, and where does each thing come from" without opening the environment manager.
   */
  import { Lock, Plus, Variable } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import type { RequestTab } from '$lib/stores/hirundo/workspace.svelte';
  import { pathParamNames, SCOPE_LABEL, usedVariables, type VarInfo } from './completions';

  let { tab, vars }: { tab: RequestTab; vars: readonly VarInfo[] } = $props();

  const paths = $derived(pathParamNames(tab.request));
  const used = $derived(
    usedVariables(tab.request).map((name) => ({ name, path: paths.includes(name), info: vars.find((v) => v.name === name) ?? null })),
  );
  const missing = $derived(used.filter((u) => !u.info && !u.path && !u.name.startsWith('$')).length);

  function define(name = '') {
    hirundoUiStore.open({ kind: 'create-variable', name, path: tab.path });
  }
</script>

<Dropdown position="fixed" direction="down" width="400px">
  {#snippet trigger({ open, toggle })}
    <IconButton tooltip="Variables this request uses" size={28} active={open} ariaHasPopup ariaExpanded={open} onclick={toggle}>
      <Variable size={14} />{#if used.length}<span class="vu-count" class:bad={missing > 0}>{used.length}</span>{/if}
    </IconButton>
  {/snippet}
  {#snippet children({ close })}
    <div class="vu">
      {#if used.length === 0}
        <EmptyState message="No variables used" description="Write {'{{'}name{'}}'} in the URL, a header or the body to use one." compact />
      {:else}
        {#each used as u (u.name)}
          <div class="vu-row">
            <code class="vu-name">{u.name}</code>
            {#if u.path}
              <Badge variant="chip" size="sm" label="path parameter" />
              <span class="vu-val">set in Params</span>
            {:else if u.info}
              <Badge variant="chip" size="sm" label={SCOPE_LABEL[u.info.scope]} />
              {#if u.info.secret}<span class="vu-val"><Lock size={11} /> secret</span>{:else if u.info.value}<span class="vu-val" title={u.info.value}>{u.info.value}</span>{:else if u.info.scope === 'dynamic'}<span class="vu-val">made up when sent</span>{/if}
            {:else}
              <Badge variant="status" size="sm" tone="warning" label="not defined" />
              <span class="vu-grow"></span>
              <Button variant="outline" size="xs" onclick={() => { close(); define(u.name); }}>Define…</Button>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  {/snippet}
  {#snippet footer({ close })}
    <div class="vu-foot"><Button variant="ghost" size="sm" block onclick={() => { close(); define(); }}>{#snippet iconStart()}<Plus size={13} />{/snippet}Add variable…</Button></div>
  {/snippet}
</Dropdown>

<style>
  .vu { display: flex; flex-direction: column; padding: 4px; max-height: 320px; overflow: auto; }
  .vu-row { display: flex; align-items: center; gap: 8px; padding: 5px 8px; min-width: 0; }
  .vu-name { font-family: var(--font-mono); font-size: var(--font-size-xs); }
  .vu-val { font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; display: inline-flex; align-items: center; gap: 4px; }
  .vu-grow { flex: 1; }
  .vu-foot { padding: 4px; }
  .vu-count { margin-left: 3px; font-size: var(--font-size-3xs); color: var(--text-secondary); }
  .vu-count.bad { color: var(--warning); }
</style>
