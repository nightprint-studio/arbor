<script lang="ts">
  /**
   * Quick insert: the syntax reference as a searchable menu — pick a token and it is written into the
   * pattern at the caret. Replaces a permanent reference pane, which took half the window to be read
   * twice a session.
   */
  import { Plus } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { REGEX_REFERENCE } from './reference';

  let { oninsert }: { oninsert: (token: string) => void } = $props();

  const items: DropdownItem[] = REGEX_REFERENCE.map((g) => ({
    kind: 'group' as const,
    id: g.title,
    label: g.title,
    collapsible: true,
    defaultCollapsed: g.title !== 'Characters',
    items: g.rows.map((r): DropdownItem => ({
      kind: 'item', id: `${g.title}:${r.token}`, label: r.token, subtitle: r.text, onclick: () => oninsert(r.token),
    })),
  }));
</script>

<Dropdown {items} searchable searchPlaceholder="Search syntax…" width="460px" emptyMessage="No such token.">
  {#snippet trigger({ toggle })}
    <Button variant="secondary" size="sm" onclick={toggle} tooltip="Insert syntax at the caret">
      <Plus size={13} /> Insert
    </Button>
  {/snippet}
</Dropdown>
