<script lang="ts">
  /** Settings › Completion › Learning — what Bennu counts and remembers about how you use its suggestions, and how to forget it. */
  import { onDestroy, onMount } from 'svelte';
  import { Sparkles } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { flcForget, flcLearnReset, flcStats, flcStatsReset, type FlcStatsReport } from '$lib/ipc/bennu/flc';
  import { statsLines } from '$lib/stores/bennu/flc-settings';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';

  const s = bennuSettingsStore;

  let forgetting = $state(false);
  let forgetBusy = $state(false);
  let stats = $state<FlcStatsReport | null>(null);
  async function refreshStats() {
    stats = await flcStats().catch(() => null);
  }
  async function resetStats() {
    await flcStatsReset().catch(() => {});
    await refreshStats();
  }
  let learnForgotten = $state(false);
  async function forgetLearned() {
    await flcLearnReset().catch(() => {});
    learnForgotten = true;
  }
  async function forgetProfile() {
    forgetBusy = true;
    await flcForget().catch(() => {});
    forgetBusy = false;
    forgetting = false;
  }
  onMount(refreshStats);
  // Asking for the stats also has the backend write pending counters, so leaving the page loses none.
  onDestroy(() => void flcStats().catch(() => {}));
  const takenLines = $derived(stats ? statsLines(stats.origins) : []);
  const takenText = $derived(takenLines.length ? takenLines.join(' · ') : 'Nothing counted yet.');
</script>

<div class="section-header">
  <h2>Learning</h2>
  <p>What Bennu counts and remembers about how you use its suggestions. Everything stays on this machine.</p>
</div>
<div class="card">
  <div class="card-section-title"><Sparkles size={12} /> Counters and habits</div>
  <FormRow label="What Bennu has learned" description="A window with everything it counted — usual arguments, what follows what, calls on a variable, what you did with its suggestions — where a row can be read and taken back. Also in the command palette.">
    <Button variant="secondary" size="sm" onclick={() => { bennuUiStore.closeSettings(); bennuUiStore.openLearned(); }}>Open…</Button>
  </FormRow>
  <FormRow label="Count how often suggestions are taken" description="Keeps numbers only — how many suggestions of each kind were shown, taken with Tab and dismissed, per confidence level, and which entry you pick from the completion list after a given type or in a given argument — in your profile, on this machine. No code is stored and nothing is sent. It is used for the summary below, for the threshold it can suggest, and to put what you pick most often first in the completion list. Forget removes the picks too.">
    <Toggle checked={s.flc.feedback} onchange={(v) => s.setFlc('feedback', v)} ariaLabel="Count how often suggestions are taken" />
  </FormRow>
  <FormRow label="Taken so far" description={takenText}>
    <Button variant="secondary" size="sm" disabled={takenLines.length === 0} onclick={resetStats}>Reset</Button>
  </FormRow>
  {#if stats?.suggested_threshold}
    <FormRow label="Suggested confidence" description={`The model's suggestions were taken at least half the time from ${stats.suggested_threshold}% up. Using it changes “Confidence needed” on the Ghost text page; nothing changes until you press the button.`}>
      <Button variant="secondary" size="sm" onclick={() => s.setFlc('minConfidence', stats?.suggested_threshold ?? s.flc.minConfidence)}>
        Use {stats.suggested_threshold}%
      </Button>
    </FormRow>
  {/if}
  <FormRow label="Learn from corrections" description="Remembers what you do with a suggestion after you take it — leave it, cut its tail, write on, change a part, only re-lay it out, or take it back — as counters per kind of suggestion, in your profile, on this machine. A suggestion you edit and then put back as it was counts as left alone. Off by default; nothing is sent anywhere.">
    <Toggle checked={s.flc.learn} onchange={(v) => { learnForgotten = false; s.setFlc('learn', v); }} ariaLabel="Learn from corrections" />
  </FormRow>
  <FormRow label="Keep the text too" description="Also keeps the suggestion and what it became, as written, in a log capped at 1 MB beside the counters — for training a model on your corrections later. Code is stored here, so it is a separate switch.">
    <Toggle checked={s.flc.learnKeepText} disabled={!s.flc.learn} onchange={(v) => s.setFlc('learnKeepText', v)} ariaLabel="Keep the text too" />
  </FormRow>
  <FormRow label="Notify when it learns" description="A notification each time a correction is counted, saying which kind of suggestion, where, what happened and how many of that kind it has seen. For checking what is being learned; no text of yours is shown. Off by default.">
    <Toggle checked={s.flc.learnNotify} disabled={!s.flc.learn} onchange={(v) => s.setFlc('learnNotify', v)} ariaLabel="Notify when it learns" />
  </FormRow>
  <FormRow label="Forget what it learned from corrections" description={learnForgotten ? 'Forgotten.' : 'Deletes those counters and the log of texts, if there is one.'}>
    <Button variant="secondary" size="sm" onclick={forgetLearned}>Forget</Button>
  </FormRow>
  <FormRow label="Remember what Bennu learned in each project" description="Keeps short pieces of your code (argument texts, annotation shapes, variable names) in your Arbor data folder, never in a repository and never sent anywhere, so the next start already knows how this project is written. Off, nothing is read or written.">
    <Toggle checked={s.flc.profileProject} onchange={(v) => s.setFlc('profileProject', v)} ariaLabel="Remember what Bennu learned in each project" />
  </FormRow>
  <FormRow label="Carry it across projects" description="Where one project has seen too little, use what your other projects taught Bennu. Stored the same way. It can bring names and habits from one project into another, so leave it off if your projects must not mix.">
    <Toggle checked={s.flc.profileGlobal} onchange={(v) => s.setFlc('profileGlobal', v)} ariaLabel="Carry it across projects" />
  </FormRow>
  <FormRow label="Forget what Bennu learned" description="Deletes the saved profiles of every project and empties what is in memory. It is learned again from the files, the next time a project is scanned.">
    <Button variant="secondary" size="sm" onclick={() => (forgetting = true)}>Forget…</Button>
  </FormRow>
</div>

{#if forgetting}
  <ConfirmModal
    title="Forget what Bennu learned?"
    message="The saved profiles of every project are deleted."
    detail="Argument habits, annotation shapes and variable names are learned again from the files, the next time a project is scanned. Your code is not touched."
    variant="danger"
    confirmLabel="Forget"
    busy={forgetBusy}
    onConfirm={forgetProfile}
    onCancel={() => (forgetting = false)}
  />
{/if}
