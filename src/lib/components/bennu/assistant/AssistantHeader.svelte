<script lang="ts">
  /**
   * The conversation you are in, and the way to another.
   *
   * Conversations are kept, so the title is also a switcher: it opens the list — today's first, each
   * with the model it used, when it was last touched and what it cost — and the line under it says the
   * open one is saved. Click the title's pencil to rename it in place.
   */
  import { ChevronDown, Check, Pencil, Trash2 } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import InlineEdit from '$lib/components/shared/ui/InlineEdit.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { formatEur, relativeTime } from './assistant-format';

  const store = bennuAssistantStore;
  const DAY = 24 * 60 * 60 * 1000;

  let renaming = $state(false);
  let draftTitle = $state('');
  // The clock "5 min ago" is read against. The panel is not a ticking one, so it is taken again only
  // when the conversations change — which is when a time could have.
  const now = $derived.by(() => {
    void store.visible;
    return Date.now();
  });

  const items = $derived.by<DropdownItem[]>(() => {
    const entry = (c: (typeof store.conversations)[number]): DropdownItem => ({
      kind: 'item',
      id: c.id,
      label: c.title,
      subtitle: `${store.models.find((m) => m.id === c.model)?.name ?? c.model} · ${relativeTime(c.updatedAt, now)}`,
      meta: formatEur(store.eur(c.cost)),
      active: c.id === store.activeId,
      onclick: () => store.select(c.id),
    });
    const recent = store.visible.filter((c) => now - c.updatedAt < DAY);
    const earlier = store.visible.filter((c) => now - c.updatedAt >= DAY);
    const out: DropdownItem[] = [];
    if (recent.length) out.push({ kind: 'group', id: 'recent', label: 'Today', items: recent.map(entry) });
    if (earlier.length) out.push({ kind: 'group', id: 'earlier', label: 'Earlier', items: earlier.map(entry) });
    return out;
  });

  function startRename() {
    if (!store.active) return;
    draftTitle = store.active.title;
    renaming = true;
  }
</script>

<div class="hd">
  {#if renaming && store.active}
    <InlineEdit
      bind:value={draftTitle}
      maxlength={120}
      onconfirm={(t) => { store.rename(store.active!.id, t); renaming = false; }}
      oncancel={() => (renaming = false)}
    />
  {:else}
    <Dropdown {items} position="fixed" direction="down" width="340px" emptyMessage="No conversations yet">
      {#snippet trigger({ open, toggle })}
        <button type="button" class="switch" class:open aria-expanded={open} aria-haspopup="menu" onclick={toggle}>
          <span class="title">{store.active?.title ?? 'Conversations'}</span>
          <ChevronDown size={13} />
        </button>
      {/snippet}
    </Dropdown>
    {#if store.active}
      <IconButton tooltip="Rename" size={22} onclick={startRename}><Pencil size={12} /></IconButton>
      <IconButton tooltip="Delete conversation" size={22} onclick={() => store.askRemove(store.active!.id)}><Trash2 size={12} /></IconButton>
    {/if}
  {/if}
</div>
{#if store.active}
  <div class="saved"><Check size={10} /> Saved · {relativeTime(store.active.updatedAt, now)}</div>
{/if}

<style>
  .hd { display: flex; align-items: center; gap: 4px; min-width: 0; }
  .switch {
    display: flex; align-items: center; gap: 6px; min-width: 0; flex: 1;
    padding: 4px 8px; margin-left: -8px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-primary); cursor: pointer; text-align: left;
    font-family: var(--font-ui-sans); font-size: var(--font-size-sm); font-weight: 600;
    transition: background var(--transition-fast);
  }
  .switch:hover, .switch.open { background: var(--bg-hover); }
  .switch :global(svg) { flex-shrink: 0; color: var(--text-muted); }
  .title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .saved {
    display: flex; align-items: center; gap: 5px; margin-top: 1px;
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .saved :global(svg) { color: var(--success); }
</style>
