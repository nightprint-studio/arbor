<script lang="ts">
  /** Settings › Completion — when the popup appears and how it matches what you type. */
  import { ListTree, Sparkles } from 'lucide-svelte';
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { MAX_STRENGTH, MIN_STRENGTH, STRENGTH_STEP } from '$lib/stores/bennu/smart-completion';

  const s = bennuSettingsStore;
  // The strength only scales the three learned signals, so it has nothing to do with all of them off.
  const learnedOff = $derived(!s.smart.nameAffinity && !s.smart.projectUsage && !s.smart.sequence);
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

<div class="card">
  <div class="card-section-title"><Sparkles size={12} /> Smart suggestions</div>
  <Callout variant="info" title="Ranking, never filtering">
    Each switch turns a signal off, not a candidate — a miss costs a place in the list, not the answer. Changes apply from the next completion.
  </Callout>
  <FormRow label="Echo the name being filled" description="Rank a candidate higher when its name says the same word as the variable or call it is for — customer for Customer customer =, customerId for findByCustomerId(.">
    <Toggle checked={s.smart.nameAffinity} onchange={(v) => s.setSmart('nameAffinity', v)} ariaLabel="Echo the name being filled" />
  </FormRow>
  <FormRow label="Order members by what this project calls" description="Counts how often the project calls each method while the index builds, and lets that decide between members that are otherwise equal. A name used once counts for nothing.">
    <Toggle checked={s.smart.projectUsage} onchange={(v) => s.setSmart('projectUsage', v)} ariaLabel="Order members by what this project calls" />
  </FormRow>
  <FormRow label="Learn call sequences" description="Rank a method higher when the project calls it right after the previous call in the chain — close() after open(), build() after the setters.">
    <Toggle checked={s.smart.sequence} onchange={(v) => s.setSmart('sequence', v)} ariaLabel="Learn call sequences" />
  </FormRow>
  <FormRow label="Strength of the learned signals" description="Scales the three switches above. 0 ignores them, 100 is as designed, higher lets them weigh more against the rest of the ordering. They can never overrule the type the position wants.">
    <NumberStepper value={s.smart.strength} min={MIN_STRENGTH} max={MAX_STRENGTH} step={STRENGTH_STEP} narrow suffix="%"
                   disabled={learnedOff}
                   onchange={(v) => s.setSmart('strength', v)} ariaLabel="Strength of the learned signals in percent" />
  </FormRow>
  <FormRow label="Chain completion" description="Also offer members of members that produce the type the position wants — order.getCustomer().getName() for a String. Listed below the direct members.">
    <Toggle checked={s.smart.chain} onchange={(v) => s.setSmart('chain', v)} ariaLabel="Chain completion" />
  </FormRow>
  <FormRow label="Let the code's shape decide" description="Setters already called on a builder sink, a local you declared and have not used yet rises, a catch leads with the exceptions its try throws, a case leads with the enum constants not yet covered.">
    <Toggle checked={s.smart.context} onchange={(v) => s.setSmart('context', v)} ariaLabel="Let the code's shape decide" />
  </FormRow>
  <FormRow label="Statement templates" description="Whole-statement completions that read the surrounding code, like iter for the nearest collection in scope. Offered only when there is something to iterate.">
    <Toggle checked={s.smart.statements} onchange={(v) => s.setSmart('statements', v)} ariaLabel="Statement templates" />
  </FormRow>
  <FormRow label="Ghost text for certain continuations" description="Draw ahead of the caret what can only be one thing — the exception variable after catch (IOException, the next uncovered enum case. Tab writes it; when in doubt nothing is drawn.">
    <Toggle checked={s.smart.ghostText} onchange={(v) => s.setSmart('ghostText', v)} ariaLabel="Ghost text for certain continuations" />
  </FormRow>
</div>
