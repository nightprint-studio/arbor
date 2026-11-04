<script lang="ts">
  /**
   * Saved searches for the entity list: pick one to run it, save what is in the box, forget one.
   * A search is the box's own text (`goblin has:Health no:Dead`), and is named by it.
   */
  import { Bookmark, BookmarkPlus, X } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { bennuBrpStore as brp } from '$lib/stores/bennu/brp.svelte';
  import { brpSavedStore as saved } from '$lib/stores/bennu/brp-saved.svelte';

  $effect(() => { void saved.load(); });

  const items = $derived.by<DropdownItem[]>(() => {
    const current = brp.search.trim();
    const rows: DropdownItem[] = [];
    if (current && !saved.queries.some((q) => q.search === current)) {
      rows.push({
        kind: 'item',
        id: 'save',
        label: 'Save this search',
        subtitle: current,
        icon: BookmarkPlus,
        onclick: () => void saved.saveQuery(current, current),
      });
    }
    for (const q of saved.queries) {
      rows.push({
        kind: 'item',
        id: `run:${q.name}`,
        label: q.name,
        subtitle: q.name === q.search ? undefined : q.search,
        icon: Bookmark,
        active: q.search === current,
        onclick: () => brp.setSearch(q.search),
      });
    }
    for (const q of saved.queries) {
      rows.push({
        kind: 'item',
        id: `forget:${q.name}`,
        label: `Forget “${q.name}”`,
        icon: X,
        danger: true,
        onclick: () => void saved.removeQuery(q.name),
      });
    }
    return rows;
  });
</script>

<Dropdown {items} position="fixed" direction="down" width="300px" emptyMessage="Type a search, then save it here.">
  {#snippet trigger({ toggle, open })}
    <IconButton tooltip="Saved searches" size={26} active={open} onclick={toggle}>
      <Bookmark size={13} />
    </IconButton>
  {/snippet}
</Dropdown>
