<script lang="ts">
  /**
   * The conversation you are in, and the way to another.
   *
   * With `onSelect` the title is a switcher: it opens the list — today's first, each with its subtitle (the
   * model, when it was last touched) and its cost at the edge — and the line under it says the open one is
   * saved. The pencil renames it in place; the bin deletes it. Without `onSelect` the title is a plain label
   * (a product that keeps one conversation per context) and only the saved line remains.
   */
  import { ChevronDown, Check, Pencil, Trash2 } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import InlineEdit from '$lib/components/shared/ui/InlineEdit.svelte';
  import { relativeTime } from './imber-format';
  import { groupConversations } from './imber-thread';
  import type { ConversationEntry } from './imber-types';

  let { title, entries = [], activeId = null, savedAt = null, now = Date.now(), onSelect, onRename, onDelete }: {
    /** The open conversation's title; falls back to "Conversations". */
    title?: string | null;
    entries?: ConversationEntry[];
    activeId?: string | null;
    /** When the open conversation was last saved; hides the "Saved" line when null. */
    savedAt?: number | null;
    /** The clock "5 min ago" is read against. */
    now?: number;
    onSelect?: (id: string) => void;
    onRename?: (id: string, title: string) => void;
    onDelete?: (id: string) => void;
  } = $props();

  let renaming = $state(false);
  let draftTitle = $state('');

  const items = $derived.by<DropdownItem[]>(() => {
    const entry = (c: ConversationEntry): DropdownItem => ({
      kind: 'item', id: c.id, label: c.title, subtitle: c.subtitle, meta: c.meta,
      active: c.id === activeId, onclick: () => onSelect?.(c.id),
    });
    const { recent, earlier } = groupConversations(entries, now);
    const out: DropdownItem[] = [];
    if (recent.length) out.push({ kind: 'group', id: 'recent', label: 'Today', items: recent.map(entry) });
    if (earlier.length) out.push({ kind: 'group', id: 'earlier', label: 'Earlier', items: earlier.map(entry) });
    return out;
  });

  function startRename() {
    draftTitle = title ?? '';
    renaming = true;
  }
</script>

<div class="hd">
  {#if renaming && activeId}
    <InlineEdit
      bind:value={draftTitle}
      maxlength={120}
      onconfirm={(t) => { onRename?.(activeId!, t); renaming = false; }}
      oncancel={() => (renaming = false)}
    />
  {:else}
    {#if onSelect}
      <Dropdown {items} position="fixed" direction="down" width="340px" emptyMessage="No conversations yet">
        {#snippet trigger({ open, toggle })}
          <button type="button" class="switch" class:open aria-expanded={open} aria-haspopup="menu" onclick={toggle}>
            <span class="title">{title ?? 'Conversations'}</span>
            <ChevronDown size={13} />
          </button>
        {/snippet}
      </Dropdown>
    {:else}
      <span class="static title">{title ?? 'Conversation'}</span>
    {/if}
    {#if activeId && onRename}<IconButton tooltip="Rename" size={22} onclick={startRename}><Pencil size={12} /></IconButton>{/if}
    {#if activeId && onDelete}<IconButton tooltip="Delete conversation" size={22} onclick={() => onDelete?.(activeId!)}><Trash2 size={12} /></IconButton>{/if}
  {/if}
</div>
{#if savedAt !== null}
  <div class="saved"><Check size={10} /> Saved · {relativeTime(savedAt, now)}</div>
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
  .static { flex: 1; font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
  .saved {
    display: flex; align-items: center; gap: 5px; margin-top: 1px;
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .saved :global(svg) { color: var(--success); }
</style>
