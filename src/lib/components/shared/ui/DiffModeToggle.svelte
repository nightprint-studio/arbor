<script lang="ts">
  /**
   * The unified / side-by-side switch for a diff — one icon button that shows the mode you would
   * move *to*, the way every editor draws it. The caller owns the state (and where it is
   * persisted); this only draws and reports the click.
   */
  import { AlignJustify, Columns2 } from 'lucide-svelte';
  import Button from './Button.svelte';

  let {
    split,
    onToggle,
    shortcut,
  }: {
    /** The diff is currently side by side. */
    split: boolean;
    onToggle: () => void;
    /** Shown in the tooltip, when the dialog binds a key for it. */
    shortcut?: string;
  } = $props();
</script>

<Button
  size="xs"
  variant="icon"
  ariaLabel={split ? 'Show a unified diff' : 'Show the two versions side by side'}
  tooltip={{ content: split ? 'Unified diff' : 'Side by side', shortcut }}
  onclick={onToggle}
>
  {#snippet iconStart()}
    {#if split}<AlignJustify size={13} />{:else}<Columns2 size={13} />{/if}
  {/snippet}
</Button>
