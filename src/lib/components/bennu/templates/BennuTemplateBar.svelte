<script lang="ts">
  /**
   * Which template a generation uses — and what is done to templates from where they are used: make one
   * the project's, create one from another, open one to change it, or read a built-in.
   *
   * Choosing here is for this generation only (`null` is the project's); "Use for this project" is
   * what persists. Starters are left out: they exist to be copied, not generated with.
   */
  import { onMount } from 'svelte';
  import { Eye, FilePlus2, PenLine, Pin } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { bennuTemplatesStore as templates } from '$lib/stores/bennu/templates.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import type { TemplateKindId } from '$lib/ipc/bennu/templates';
  import BennuNewTemplateModal from './BennuNewTemplateModal.svelte';

  let {
    kind,
    value,
    onchange,
  }: { kind: TemplateKindId; value: string | null; onchange: (next: string | null) => void } = $props();

  onMount(() => { void templates.load(kind); });

  const info = $derived(templates.of(kind));
  // Starters are copied, not generated with; a template the project does not meet is not offered here.
  const usable = $derived(info?.templates.filter((t) => !t.starter && !t.unmet) ?? []);
  const projectTemplate = $derived(info?.project ?? usable[0]?.name ?? '');
  const selected = $derived(value ?? projectTemplate);
  const options = $derived(
    usable.map((t) => ({ value: t.name, label: t.origin === 'builtin' ? `${t.name} (built-in)` : t.name })),
  );
  const editable = $derived(usable.find((t) => t.name === selected)?.path ?? null);

  let creating = $state(false);
</script>

<div class="bar">
  <div class="select">
    <Select
      value={selected}
      {options}
      size="sm"
      ariaLabel="Template"
      onchange={(next) => onchange(next === projectTemplate ? null : next)}
    />
  </div>
  {#if projectStore.project}
    <Button
      variant="ghost"
      size="xs"
      disabled={!selected || selected === projectTemplate}
      tooltip="Generate with this template in this project from now on"
      onclick={() => void templates.setProject(kind, selected).then(() => onchange(null))}
    >
      {#snippet iconStart()}<Pin size={12} />{/snippet}
      {selected === projectTemplate ? "The project's template" : 'Use for this project'}
    </Button>
  {/if}
  <Button variant="ghost" size="xs" tooltip="Create a template from this one, and open it" onclick={() => (creating = true)}>
    {#snippet iconStart()}<FilePlus2 size={12} />{/snippet}
    New template…
  </Button>
  {#if editable}
    <Button variant="ghost" size="xs" tooltip="Open this template in the editor" onclick={() => void templates.edit(kind, selected)}>
      {#snippet iconStart()}<PenLine size={12} />{/snippet}
      Edit
    </Button>
  {:else if selected}
    <Button variant="ghost" size="xs" tooltip="Open this built-in template read-only in the editor" onclick={() => void templates.edit(kind, selected)}>
      {#snippet iconStart()}<Eye size={12} />{/snippet}
      View
    </Button>
  {/if}
</div>

{#if creating}
  <BennuNewTemplateModal
    {kind}
    from={selected || null}
    onClose={() => (creating = false)}
    onCreated={(name) => onchange(name)}
  />
{/if}

<style>
  .bar { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .select { min-width: 180px; }
</style>
