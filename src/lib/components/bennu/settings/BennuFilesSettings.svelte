<script lang="ts">
  /**
   * Settings › Files — what happens to a buffer on its way to disk, and what is kept of what it
   * used to be.
   *
   * Split out of the Editor page because it answers a different question: not how the file is
   * drawn, but what becomes of it.
   */
  import { History, Save } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';

  const s = bennuSettingsStore;
</script>

<div class="section-header">
  <h2>Files</h2>
  <p>Saving, and the record of what a file used to be.</p>
</div>

<div class="card">
  <div class="card-section-title"><Save size={12} /> Saving</div>
  <FormRow label="Autosave" description="Write a modified file to disk automatically — after a short idle, when you switch tabs, and when the window loses focus. Off saves only on Ctrl+S.">
    <Toggle checked={s.autosave} onchange={(v) => s.setAutosave(v)} ariaLabel="Autosave" />
  </FormRow>
</div>

<div class="card">
  <div class="card-section-title"><History size={12} /> Local history</div>
  <FormRow label="Local history" description="Keep a private record of what every project file used to be, so a save, a refactor or a delete can be undone long after the editor's own undo has moved on. Stored in Arbor's data folder, never inside the project. Files git ignores are skipped.">
    <Toggle checked={s.localHistory} onchange={(v) => s.setLocalHistory(v)} ariaLabel="Local history" />
  </FormRow>
  {#if s.localHistory}
    <FormRow label="Keep history for" description="Labelled revisions, and each file's newest one, are kept regardless — a label is a promise, and a file whose only revision aged out would stop having a history exactly when it is the last copy.">
      <NumberStepper value={s.localHistoryDays} min={1} max={90} narrow suffix="days"
                     onchange={(v) => s.setLocalHistoryDays(v)} ariaLabel="Days of local history" />
    </FormRow>
    <FormRow label="History size limit" description="Per project. Over it, the oldest revisions go first.">
      <NumberStepper value={s.localHistoryMaxMb} min={16} max={4096} step={16} narrow suffix="MB"
                     onchange={(v) => s.setLocalHistoryMaxMb(v)} ariaLabel="Local history size limit" />
    </FormRow>
    <FormRow label="Skip files larger than" description="One large binary would spend the whole budget on a single revision that no diff can show anyway.">
      <NumberStepper value={s.localHistoryMaxFileMb} min={1} max={128} narrow suffix="MB"
                     onchange={(v) => s.setLocalHistoryMaxFileMb(v)} ariaLabel="Local history file size ceiling" />
    </FormRow>
  {/if}
</div>
