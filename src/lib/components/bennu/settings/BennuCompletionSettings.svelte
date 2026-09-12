<script lang="ts">
  /** Settings › Completion — when the popup appears and how it matches what you type. */
  import { ListTree } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';

  const s = bennuSettingsStore;
</script>

<div class="section-header">
  <h2>Completion</h2>
  <p>When the completion popup appears and how it matches what you type.</p>
</div>
<div class="card">
  <div class="card-section-title"><ListTree size={12} /> Popup</div>
  <FormRow label="Auto-popup on typing" description="Show suggestions automatically as you type an identifier.">
    <Toggle checked={s.autoPopup} onchange={(v) => s.setAutoPopup(v)} ariaLabel="Auto-popup on typing" />
  </FormRow>
  <FormRow label="Popup delay" description="How long to wait before the auto-popup opens.">
    <NumberStepper value={s.popupDelayMs} min={0} max={2000} step={50} narrow suffix="ms"
                   disabled={!s.autoPopup}
                   onchange={(v) => s.setPopupDelayMs(v)} ariaLabel="Popup delay in milliseconds" />
  </FormRow>
  <FormRow label="Case-sensitive matching" description="Require the prefix's case to match the candidate. Abbreviations are matched whatever case you type them in either way — the word is the template's name, not a symbol in your code.">
    <Toggle checked={s.caseSensitive} onchange={(v) => s.setCaseSensitive(v)} ariaLabel="Case-sensitive matching" />
  </FormRow>
  <FormRow label="Auto-import on accept" description="Add the missing import when you accept a completion.">
    <Toggle checked={s.autoImport} onchange={(v) => s.setAutoImport(v)} ariaLabel="Auto-import on accept" />
  </FormRow>
  <FormRow
    label="Order type names by what this project imports"
    description="Counts how many files import each candidate and lets that decide between types with the same name — so List in a project that uses java.util.List everywhere stops offering the one from a jar nobody here has named. Java only, counted during the index build. Off stops the counts being consulted at once, not at the next rebuild; what the file itself imports, and its own package, come first either way."
  >
    <Toggle checked={s.importCensus} onchange={(v) => s.setImportCensus(v)} ariaLabel="Order type names by what this project imports" />
  </FormRow>
</div>
