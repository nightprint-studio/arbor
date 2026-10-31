<script lang="ts">
  /** Settings › Completion › Smart suggestions — which signals order the popup, and how strongly. */
  import { Sparkles } from 'lucide-svelte';
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
  <h2>Smart suggestions</h2>
  <p>What decides the order of the completion list, beyond the match with what you typed.</p>
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
  <FormRow label="Animate the popup and ghost text" description="A very short fade when the completion list opens, and ghost text uncovered from left to right as it appears. Nothing is delayed: Tab takes the whole line from the first instant. Follows the app's animation setting, so it does nothing when animations are off.">
    <Toggle checked={s.smart.motion} onchange={(v) => s.setSmart('motion', v)} ariaLabel="Animate the popup and ghost text" />
  </FormRow>
</div>
