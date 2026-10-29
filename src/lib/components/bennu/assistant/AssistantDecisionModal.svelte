<script lang="ts">
  /**
   * How much this conversation decides for itself, and what you want it to know about decisions.
   *
   * The level can follow the setting or be this conversation's own; the text is added to the global
   * instructions about decisions. Both apply from the next message and are kept with the conversation.
   */
  import { Scale } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { DECISION_LEVELS, MAX_DECISION_NOTES, decisionInfo } from './assistant-decision';

  const store = bennuAssistantStore;

  const options = $derived([
    { value: '', label: 'Follow the setting', description: `Now: ${decisionInfo(store.decisionLevel).label}` },
    ...DECISION_LEVELS.map((l) => ({ value: l.id, label: l.label, description: l.description })),
  ]);

  const autoOptions = $derived([
    { value: '', label: 'Follow the setting', description: `Now: ${store.autoContinueOnStop ? 'on' : 'off'}` },
    { value: 'on', label: 'Start a run', description: 'When the program it is debugging stops while it is idle, look at it right away (at most 3 an hour).' },
    { value: 'off', label: 'Wait for me', description: 'Show a card and queue the snapshot for my next message.' },
  ]);
</script>

{#if store.decisionOpen}
  <Modal onClose={() => store.closeDecision()} width="600px" height="620px" ariaLabel="Decisions in this conversation">
    {#snippet header()}
      <ModalHeader onClose={() => store.closeDecision()}>
        <Scale size={14} />
        <span class="title">Decisions in this conversation</span>
      </ModalHeader>
    {/snippet}

    <div class="body">
      <FormRow stacked label="Level" description="Applies from your next message.">
        <RadioGroup value={store.chatDecisionLevel} {options} appearance="card" size="sm" block onchange={(v) => store.setChatDecisionLevel(v)} />
      </FormRow>
      <FormRow stacked label="Your instructions about decisions" description="Added to the ones in Settings › Imber. For example: always ask before touching public API.">
        <Textarea framed maxLength={MAX_DECISION_NOTES} bind:value={() => store.chatDecisionNotes, (v) => store.setChatDecisionNotes(v)} placeholder="Nothing extra for this conversation" minRows={3} maxRows={8} ariaLabel="Instructions about decisions for this conversation" />
      </FormRow>
      <FormRow stacked label="When its debugged program stops" description="Spends money and counts toward your limits, so it is off unless you turned it on in Settings › Imber or here.">
        <RadioGroup value={store.chatAutoContinue} options={autoOptions} appearance="card" size="sm" block onchange={(v) => store.setChatAutoContinue(v)} />
      </FormRow>
    </div>

    {#snippet footer()}
      <ModalFooter align="end">
        <Button variant="primary" size="sm" onclick={() => store.closeDecision()}>Done</Button>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .body { display: flex; flex-direction: column; gap: 14px; padding: 14px 16px; }
</style>
