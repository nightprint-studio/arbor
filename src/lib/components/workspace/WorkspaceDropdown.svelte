<script lang="ts">
  import { slide } from 'svelte/transition';
  import { ChevronDown, ChevronRight, Plus, Settings2, FolderGit2, Check } from 'lucide-svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { SCRATCH_ID, workspaceColorVar } from '$lib/types/workspace';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';

  interface Props {
    onManage: () => void;
    onCreate: () => void;
  }
  let { onManage, onCreate }: Props = $props();

  const active = $derived(workspacesStore.active);

  function filterEntries(raw: typeof workspacesStore.grouped, q: string) {
    if (!q) return raw;
    const out: typeof raw = [];
    for (const e of raw) {
      if (e.kind === 'group') {
        const hits = e.children.filter(c => c.name.toLowerCase().includes(q));
        if (hits.length) out.push({ kind: 'group', group: e.group, children: hits });
      } else if (e.ws.name.toLowerCase().includes(q)) {
        out.push(e);
      }
    }
    return out;
  }
</script>

<Dropdown
  position="fixed"
  direction="down"
  searchable={true}
  searchPlaceholder="Find workspace…"
  width="300px"
>
  {#snippet trigger({ open, toggle })}
    <button
      class="ws-trigger"
      class:open
      onclick={toggle}
      use:tooltip={active ? `Workspace: ${active.name}` : 'Select workspace'}
      aria-haspopup="menu"
      aria-expanded={open}
    >
      {#if active}
        <Monogram name={active.name} color={workspaceColorVar(active.color_idx)} size={22} />
        <span class="ws-trigger-name">{active.name}</span>
      {:else}
        <FolderGit2 size={14} class="ws-trigger-idle" />
        <span class="ws-trigger-name muted">Workspace</span>
      {/if}
      <ChevronDown size={12} class="ws-trigger-chev" />
    </button>
  {/snippet}

  {#snippet children({ filter, close, reposition })}
    {@const q = filter.trim().toLowerCase()}
    {@const entries = filterEntries(workspacesStore.grouped, q)}

    {#if entries.length === 0}
      <div class="ws-empty">No workspace matches "{filter}"</div>
    {/if}

    {#each entries as entry (entry.kind === 'group' ? `g:${entry.group.id}` : `w:${entry.ws.id}`)}
      {#if entry.kind === 'group'}
        <button
          class="ws-group-row"
          onclick={(e) => { e.stopPropagation(); workspacesStore.toggleGroupCollapsed(entry.group.id); reposition(); }}
        >
          {#if entry.group.collapsed}
            <ChevronRight size={13} />
          {:else}
            <ChevronDown size={13} />
          {/if}
          <Monogram name={entry.group.name} color={workspaceColorVar(entry.group.color_idx)} size={18} />
          <span class="ws-group-name">{entry.group.name}</span>
          <span class="ws-group-count">{entry.children.length}</span>
        </button>

        {#if !entry.group.collapsed}
          <div transition:slide={{ duration: animStore.dBase }}>
            {#each entry.children as ws (ws.id)}
              {@const isActive = ws.id === workspacesStore.activeId}
              <button
                class="ws-row ws-row-child"
                class:active={isActive}
                onclick={() => { close(); if (ws.id !== workspacesStore.activeId) workspacesStore.setActive(ws.id); }}
                role="menuitem"
              >
                <Monogram name={ws.name} color={workspaceColorVar(ws.color_idx)} size={18} />
                <span class="ws-row-name">{ws.name}</span>
                {#if isActive}
                  <Check size={11} class="ws-row-check" />
                {:else}
                  <span class="ws-row-count">{ws.repo_ids.length}</span>
                {/if}
              </button>
            {/each}
          </div>
        {/if}

      {:else}
        {@const ws = entry.ws}
        {@const isActive = ws.id === workspacesStore.activeId}
        <button
          class="ws-row"
          class:active={isActive}
          class:scratch={ws.id === SCRATCH_ID}
          onclick={() => { close(); if (ws.id !== workspacesStore.activeId) workspacesStore.setActive(ws.id); }}
          role="menuitem"
        >
          <Monogram name={ws.name} color={workspaceColorVar(ws.color_idx)} size={18} />
          <span class="ws-row-name">{ws.name}</span>
          {#if isActive}
            <Check size={11} class="ws-row-check" />
          {:else}
            <span class="ws-row-count">{ws.repo_ids.length}</span>
          {/if}
        </button>
      {/if}
    {/each}
  {/snippet}

  {#snippet footer({ close })}
    <button class="ws-foot-item" onclick={() => { close(); onCreate(); }} role="menuitem">
      <Plus size={13} />
      <span>New Workspace</span>
    </button>
    <button class="ws-foot-item" onclick={() => { close(); onManage(); }} role="menuitem">
      <Settings2 size={13} />
      <span>Manage…</span>
    </button>
  {/snippet}
</Dropdown>

<style>
  /* ── Trigger ─────────────────────────────────────────────────────────────── */
  .ws-trigger {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    font-weight: 500;
    transition: background var(--transition-fast), border-color var(--transition-fast);
    -webkit-app-region: no-drag;
    max-width: 220px;
  }
  .ws-trigger:hover    { background: var(--bg-hover); }
  .ws-trigger.open     { background: var(--bg-hover); border-color: var(--border-subtle); }
  .ws-trigger-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ws-trigger-name.muted { color: var(--text-muted); }
  :global(.ws-trigger .ws-trigger-chev)       { color: var(--text-muted); transition: color var(--transition-fast); }
  :global(.ws-trigger:hover .ws-trigger-chev) { color: var(--text-secondary); }
  :global(.ws-trigger .ws-trigger-idle)       { color: var(--text-muted); }

  /* ── List items ──────────────────────────────────────────────────────────── */
  .ws-empty {
    padding: 18px 12px;
    font-size: 11px;
    color: var(--text-muted);
    text-align: center;
  }

  .ws-row, .ws-group-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    text-align: left;
    color: var(--text-primary);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    transition: background var(--transition-fast);
  }
  .ws-row:hover, .ws-group-row:hover { background: var(--bg-hover); }
  .ws-row.active { background: color-mix(in srgb, var(--accent) 8%, transparent); }
  .ws-row-child  { padding-left: 33px; }
  .ws-row-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }
  .ws-row-count {
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 2px 7px;
    border-radius: 9px;
    font-variant-numeric: tabular-nums;
    line-height: 1.4;
  }
  :global(.ws-row-check) { color: var(--accent); flex-shrink: 0; }
  .ws-row.scratch .ws-row-name { color: var(--text-secondary); }

  .ws-group-row {
    padding: 7px 10px;
    color: var(--text-secondary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 600;
  }
  .ws-group-name  { flex: 1; }
  .ws-group-count {
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-md);
  }

  /* ── Footer items ────────────────────────────────────────────────────────── */
  .ws-foot-item {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 7px 9px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    text-align: left;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ws-foot-item:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
