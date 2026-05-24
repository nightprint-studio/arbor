<script lang="ts">
  import { diffStore } from '$lib/stores/diff.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import type { DiffMode } from '$lib/types/config';

  // All values are read straight from the store ($derived so external writes
  // — e.g. command palette toggles — flow back into the UI without effects).
  const contextLines   = $derived(diffStore.contextLines);
  const confirmDiscard = $derived(diffStore.confirmDiscard);
  const diffMode       = $derived(diffStore.mode);
  const fullFile       = $derived(diffStore.fullFile);
  const virtThreshold  = $derived(diffStore.virtThreshold);
  const tabWidth       = $derived(diffStore.tabWidth);

  const TAB_WIDTH_OPTIONS = [
    { value: '2', label: '2' },
    { value: '4', label: '4' },
    { value: '8', label: '8' },
  ];
</script>

<SectionHeader title="Diff &amp; Stage" description="Configure how code diffs are computed, rendered, and how the stage area behaves." />

<div class="card">
  <!-- Diff algorithm: hidden from UI for now.
  <FormRow label="Diff algorithm" description="Controls how changes are detected">
    <Select
      value={diffAlgorithm}
      onchange={(v) => diffStore.setAlgorithm(v as DiffAlgorithm)}
      options={[
        { value: 'myers',     label: 'Myers (default)' },
        { value: 'patience',  label: 'Patience' },
        { value: 'histogram', label: 'Histogram' },
      ]}
    />
  </FormRow>
  -->

  <FormRow label="Context lines" description="Lines shown around each changed hunk">
    <NumberStepper
      value={contextLines}
      onchange={(n) => diffStore.setContextLines(n)}
      min={0}
      max={20}
      ariaLabel="Context lines"
    />
  </FormRow>

  <FormRow label="View mode" description="How the diff is laid out">
    <RadioGroup
      value={diffMode}
      onchange={(v) => diffStore.setMode(v as DiffMode)}
      options={[
        { value: 'unified', label: 'Unified' },
        { value: 'split',   label: 'Split' },
      ]}
      appearance="segment"
    />
  </FormRow>

  <FormRow label="Tab width" description="Visual width of a Tab character in diff lines.">
    <RadioGroup
      value={String(tabWidth)}
      onchange={(v) => diffStore.setTabWidth(parseInt(v, 10))}
      options={TAB_WIDTH_OPTIONS}
      appearance="segment"
    />
  </FormRow>

  <!-- Word wrap: hidden from UI for now.
  <FormRow label="Word wrap" description="Wrap long lines in the diff viewer">
    <Toggle checked={wordWrap} onchange={(v) => diffStore.setWordWrap(v)} />
  </FormRow>
  -->

  <FormRow label="Show full file" description="Render the entire file with diff highlights, not just changed hunks. Useful for navigating around a change in context.">
    <Toggle checked={fullFile} onchange={(v) => diffStore.setFullFile(v)} />
  </FormRow>

  <FormRow label="Virtualization threshold" description="When a file's diff has more than this many lines, switch to a virtualized renderer that only paints visible rows. Lower keeps large files snappy; word wrap forces the simple renderer regardless.">
    <NumberStepper
      value={virtThreshold}
      onchange={(n) => diffStore.setVirtThreshold(n)}
      min={50}
      max={100000}
      step={50}
      ariaLabel="Virtualization threshold"
    />
  </FormRow>
</div>

<div class="card">
  <FormRow label="Confirm before discarding" description="Show a confirmation dialog before discarding file changes">
    <Toggle checked={confirmDiscard} onchange={(v) => diffStore.setConfirmDiscard(v)} />
  </FormRow>
</div>
