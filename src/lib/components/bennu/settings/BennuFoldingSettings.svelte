<script lang="ts">
  /** Settings › Folding — the fold controls in the gutter. */
  import { FoldVertical } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';

  const s = bennuSettingsStore;
</script>

<div class="section-header">
  <h2>Folding</h2>
  <p>Code-folding behaviour in the editor gutter.</p>
</div>
<div class="card">
  <div class="card-section-title"><FoldVertical size={12} /> Regions</div>
  <FormRow label="Enable code folding" description="Show fold controls for methods, classes and blocks.">
    <Toggle checked={s.foldingEnabled} onchange={(v) => s.setFoldingEnabled(v)} ariaLabel="Enable code folding" />
  </FormRow>
  <FormRow label="Fold block comments by default" description="Collapse /* … */ comments when a file opens.">
    <Toggle checked={s.foldBlockComments} disabled={!s.foldingEnabled}
            onchange={(v) => s.setFoldBlockComments(v)} ariaLabel="Fold block comments by default" />
  </FormRow>
</div>
