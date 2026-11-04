<script lang="ts">
  /**
   * MethodSelect — the HTTP method picker of the URL bar. Every method keeps its own colour (the one the
   * collections tree and the tabs use), in the trigger and in the list, so the colour is a property of the
   * method and not of whatever is selected.
   */
  import { ChevronDown } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { METHODS, type Method } from '$lib/types/hirundo';
  import { methodColor } from '../request-model';

  let { value, onchange }: { value: Method; onchange: (m: Method) => void } = $props();

  const items = $derived<DropdownItem[]>(
    METHODS.map((m) => ({ kind: 'item', id: m, label: m, labelColor: methodColor(m), active: m === value, onclick: () => onchange(m) })),
  );
</script>

<Dropdown {items} width="150px" direction="down" position="fixed">
  {#snippet trigger({ open, toggle })}
    <Button variant="outline" ariaExpanded={open} ariaHaspopup="menu" ariaLabel="HTTP method" onclick={toggle}>
      <span class="ms-name" style:color={methodColor(value)}>{value}</span>
      <ChevronDown size={12} />
    </Button>
  {/snippet}
</Dropdown>

<style>
  .ms-name { font: 700 var(--font-size-sm) var(--font-mono); min-width: 58px; text-align: left; }
</style>
