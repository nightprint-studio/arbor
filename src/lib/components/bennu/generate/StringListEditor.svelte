<script lang="ts">
  /**
   * An editable list of single-line strings — CORS origins, systems a plugin registers, resources a system
   * takes. Enter in a row adds the next one; an empty row is dropped when the list is read by the backend.
   */
  import { Plus, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';

  let {
    items = $bindable<string[]>([]),
    placeholder = '',
    addLabel = 'Add',
    ariaLabel,
  }: {
    items?: string[];
    placeholder?: string;
    addLabel?: string;
    ariaLabel: string;
  } = $props();

  let listEl = $state<HTMLElement | null>(null);

  function focusRow(index: number) {
    queueMicrotask(() => listEl?.querySelectorAll<HTMLInputElement>('input')[index]?.focus());
  }

  function add(after = items.length - 1) {
    const next = [...items];
    next.splice(after + 1, 0, '');
    items = next;
    focusRow(after + 1);
  }

  function remove(index: number) {
    items = items.filter((_, i) => i !== index);
  }

  function onKey(e: KeyboardEvent, index: number) {
    if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      add(index);
    } else if (e.key === 'Backspace' && items[index] === '' && items.length > 0) {
      e.preventDefault();
      remove(index);
      focusRow(Math.max(0, index - 1));
    }
  }
</script>

<div class="sle" bind:this={listEl}>
  {#each items as _, i (i)}
    <div class="sle-row">
      <Input
        block
        size="sm"
        {placeholder}
        ariaLabel={`${ariaLabel} ${i + 1}`}
        bind:value={items[i]}
        onkeydown={(e) => onKey(e, i)}
      />
      <IconButton tooltip="Remove" size={12} onclick={() => remove(i)}><X size={12} /></IconButton>
    </div>
  {/each}
  <div>
    <Button variant="ghost" size="sm" onclick={() => add()}>
      {#snippet iconStart()}<Plus size={12} />{/snippet}
      {addLabel}
    </Button>
  </div>
</div>

<style>
  .sle { display: flex; flex-direction: column; gap: 5px; }
  .sle-row { display: flex; align-items: center; gap: 4px; }
  .sle-row > :global(.input-wrap) { flex: 1; min-width: 0; }
</style>
