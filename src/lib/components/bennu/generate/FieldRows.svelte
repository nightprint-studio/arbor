<script lang="ts">
  /**
   * Editable rows of a name and a type — struct fields, query parameters, columns, state. The same rows
   * edit anything a generator takes as a `Vec<Field>`; `typed={false}` drops the type column (state variants).
   */
  import { Plus, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';

  interface Row { name: string; ty: string }

  let {
    items = $bindable<Row[]>([]),
    namePlaceholder = 'name',
    typePlaceholder = 'Type',
    addLabel = 'Field',
    ariaLabel,
    typed = true,
    defaultType = 'String',
  }: {
    items?: Row[];
    namePlaceholder?: string;
    typePlaceholder?: string;
    addLabel?: string;
    ariaLabel: string;
    typed?: boolean;
    defaultType?: string;
  } = $props();
</script>

<div class="fr">
  {#each items as row, i (i)}
    <div class="fr-row" class:single={!typed}>
      <Input size="sm" block placeholder={namePlaceholder} ariaLabel={`${ariaLabel} ${i + 1} name`} bind:value={row.name} />
      {#if typed}
        <Input size="sm" block placeholder={typePlaceholder} ariaLabel={`${ariaLabel} ${i + 1} type`} bind:value={row.ty} />
      {/if}
      <IconButton tooltip="Remove" size={12} onclick={() => (items = items.filter((_, n) => n !== i))}><X size={12} /></IconButton>
    </div>
  {/each}
  <div>
    <Button variant="ghost" size="sm" onclick={() => (items = [...items, { name: '', ty: typed ? defaultType : '' }])}>
      {#snippet iconStart()}<Plus size={12} />{/snippet}
      {addLabel}
    </Button>
  </div>
</div>

<style>
  .fr { display: flex; flex-direction: column; gap: 5px; }
  .fr-row { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.3fr) auto; gap: 4px; align-items: center; }
  .fr-row.single { grid-template-columns: minmax(0, 1fr) auto; }
</style>
