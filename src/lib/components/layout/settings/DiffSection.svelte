<script lang="ts">
  import { diffStore } from '$lib/stores/diff.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';

  type DiffAlgo = 'myers' | 'patience' | 'minimal';
  function loadDiffAlgo(): DiffAlgo {
    const v = localStorage.getItem('arbor:diff-algo');
    return (v === 'myers' || v === 'patience' || v === 'minimal') ? v : 'myers';
  }
  function loadContextLines(): number {
    const n = parseInt(localStorage.getItem('arbor:context-lines') ?? '');
    return Number.isFinite(n) && n >= 0 && n <= 20 ? n : 3;
  }
  let diffAlgorithm = $state<DiffAlgo>(loadDiffAlgo());
  let contextLines = $state(loadContextLines());
  let confirmDiscard = $state((localStorage.getItem('arbor:confirm-discard') ?? 'true') === 'true');

  // These are driven from the store (source of truth)
  let diffMode = $state(diffStore.mode);
  let wordWrap = $state(diffStore.wordWrap);
  let fullFile = $state(diffStore.fullFile);
  let virtThreshold = $state(diffStore.virtThreshold);

  $effect(() => { localStorage.setItem('arbor:diff-algo', diffAlgorithm); });
  $effect(() => { localStorage.setItem('arbor:context-lines', String(contextLines)); });
  $effect(() => { localStorage.setItem('arbor:confirm-discard', String(confirmDiscard)); });
  $effect(() => { diffStore.setMode(diffMode); });
  $effect(() => { diffStore.setWordWrap(wordWrap); });
  $effect(() => { diffStore.setFullFile(fullFile); });
  $effect(() => { diffStore.setVirtThreshold(virtThreshold); });
</script>

<SectionHeader title="Diff &amp; Stage" description="Configure how code diffs are computed, rendered, and how the stage area behaves." />

<div class="card">
  <FormRow label="Diff algorithm" description="Controls how changes are detected">
    <Select
      bind:value={diffAlgorithm}
      options={[
        { value: 'myers', label: 'Myers (default)' },
        { value: 'patience', label: 'Patience' },
        { value: 'minimal', label: 'Minimal' },
      ]}
    />
  </FormRow>

  <FormRow label="Context lines" description="Lines shown around each changed hunk">
    <NumberStepper bind:value={contextLines} min={0} max={20} ariaLabel="Context lines" />
  </FormRow>

  <FormRow label="View mode" description="How the diff is laid out">
    <RadioGroup
      bind:value={diffMode}
      options={[
        { value: 'unified', label: 'Unified' },
        { value: 'split', label: 'Split' },
      ]}
      appearance="segment"
    />
  </FormRow>

  <FormRow label="Word wrap" description="Wrap long lines in the diff viewer">
    <Toggle bind:checked={wordWrap} />
  </FormRow>

  <FormRow label="Show full file" description="Render the entire file with diff highlights, not just changed hunks. Useful for navigating around a change in context.">
    <Toggle bind:checked={fullFile} />
  </FormRow>

  <FormRow label="Virtualization threshold" description="When a file's diff has more than this many lines, switch to a virtualized renderer that only paints visible rows. Lower keeps large files snappy; word wrap forces the simple renderer regardless.">
    <NumberStepper bind:value={virtThreshold} min={50} max={100000} step={50} ariaLabel="Virtualization threshold" />
  </FormRow>
</div>

<div class="card">
  <FormRow label="Confirm before discarding" description="Show a confirmation dialog before discarding file changes">
    <Toggle bind:checked={confirmDiscard} />
  </FormRow>
</div>
