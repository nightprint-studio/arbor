<script lang="ts">
  /**
   * The saved games, one click from the panel header: pick one to switch to it, or save the one you
   * are connected to. The inspector holds one connection at a time, so switching replaces it.
   */
  import { BookmarkPlus, Server } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { bennuBrpStore as brp } from '$lib/stores/bennu/brp.svelte';
  import { brpSavedStore as saved } from '$lib/stores/bennu/brp-saved.svelte';

  const current = $derived(brp.status.endpoint);

  $effect(() => { void saved.load(); });

  const items = $derived.by<DropdownItem[]>(() => {
    const rows: DropdownItem[] = saved.endpoints.map((e) => ({
      kind: 'item',
      id: e.endpoint,
      label: e.name,
      subtitle: e.endpoint,
      active: e.endpoint === current,
      onclick: () => void brp.connect(e.endpoint),
    }));
    if (current && !saved.isEndpointSaved(current)) {
      rows.push({
        kind: 'item',
        id: 'save-current',
        label: 'Save this game',
        subtitle: current,
        icon: BookmarkPlus,
        onclick: () => void saved.saveEndpoint(current),
      });
    }
    return rows;
  });
</script>

{#if items.length > 0}
  <Dropdown {items} position="fixed" direction="down" width="280px">
    {#snippet trigger({ toggle, open })}
      <IconButton tooltip="Saved games" size={22} active={open} onclick={toggle}>
        <Server size={13} />
      </IconButton>
    {/snippet}
  </Dropdown>
{/if}
