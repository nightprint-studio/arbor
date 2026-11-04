<script lang="ts">
  /**
   * "Copy as…" for a value the inspector shows: the game's JSON, or the same data laid out as RON
   * (see `brp-format.ts` for what that does and does not promise).
   */
  import { Copy } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { toJson, toRon } from './brp-format';

  let {
    value,
    what,
  }: {
    /** Read at click time, so a value that changed while the menu was closed is what gets copied. */
    value: () => unknown;
    /** For the tooltip and the toast: "entity", "resource". */
    what: string;
  } = $props();

  const items = $derived<DropdownItem[]>([
    { kind: 'item', id: 'json', label: 'Copy as JSON', onclick: () => void copyToClipboard(toJson(value()), { successToast: `Copied the ${what} as JSON`, errorToast: true }) },
    { kind: 'item', id: 'ron', label: 'Copy as RON', onclick: () => void copyToClipboard(toRon(value()), { successToast: `Copied the ${what} as RON`, errorToast: true }) },
  ]);
</script>

<Dropdown {items} position="fixed" direction="down" width="180px">
  {#snippet trigger({ toggle, open })}
    <IconButton tooltip={`Copy the ${what}…`} size={22} active={open} onclick={toggle}>
      <Copy size={13} />
    </IconButton>
  {/snippet}
</Dropdown>
