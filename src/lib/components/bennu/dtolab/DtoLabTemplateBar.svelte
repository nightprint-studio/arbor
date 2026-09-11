<script lang="ts">
  /**
   * Which template a generation uses — and the three things done to templates: make one the project's,
   * create one from another, and open one to change it.
   *
   * Choosing a template here is for this generation only; "Use for this project" is what persists.
   */
  import { FilePlus2, PenLine, Pin } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { bennuDtoLabStore as lab } from '$lib/stores/bennu/dtolab.svelte';
  import DtoLabNewTemplateModal from './DtoLabNewTemplateModal.svelte';

  const info = $derived(lab.templates);
  const projectTemplate = $derived(info?.project ?? lab.view?.template ?? 'default');
  const selected = $derived(lab.template ?? projectTemplate);
  const options = $derived(
    (info?.templates ?? [{ name: 'default', origin: 'builtin' as const, path: null }]).map((t) => ({
      value: t.name,
      label: t.origin === 'builtin' ? `${t.name} (built-in)` : t.name,
    })),
  );
  const editable = $derived(info?.templates.find((t) => t.name === selected)?.path ?? null);

  let creating = $state(false);
</script>

<div class="bar">
  <div class="select">
    <Select
      value={selected}
      {options}
      size="sm"
      ariaLabel="Template"
      onchange={(value) => lab.setTemplate(value === projectTemplate ? null : value)}
    />
  </div>
  <Button
    variant="ghost"
    size="xs"
    disabled={selected === projectTemplate}
    tooltip="Generate with this template in this project from now on"
    onclick={() => void lab.setProjectTemplate(selected)}
  >
    {#snippet iconStart()}<Pin size={12} />{/snippet}
    {selected === projectTemplate ? "The project's template" : 'Use for this project'}
  </Button>
  <Button variant="ghost" size="xs" tooltip="Create a template from this one, and open it" onclick={() => (creating = true)}>
    {#snippet iconStart()}<FilePlus2 size={12} />{/snippet}
    New template…
  </Button>
  {#if editable}
    <Button variant="ghost" size="xs" tooltip="Open this template in the editor" onclick={() => void lab.editTemplate(selected)}>
      {#snippet iconStart()}<PenLine size={12} />{/snippet}
      Edit
    </Button>
  {/if}
</div>

{#if creating}
  <DtoLabNewTemplateModal from={selected} onClose={() => (creating = false)} />
{/if}

<style>
  .bar { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .select { min-width: 180px; }
</style>
