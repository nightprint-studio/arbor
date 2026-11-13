<script lang="ts">
  /**
   * The account's credit balance, entered by hand: the API does not report it, so the user copies it from the Claude
   * Console and Imber estimates what is left from what is spent after it. Saving records the moment too — the
   * estimate subtracts only what came later — so entering the same figure again restarts the count.
   */
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { imberCreditsStore } from '$lib/stores/imber-credits.svelte';

  const s = bennuAssistantStore;
  let draft = $state('');

  const amount = $derived(Number(draft.replace(',', '.')));
  const valid = $derived(draft.trim() !== '' && Number.isFinite(amount) && amount > 0);
  const entered = $derived(s.limits.credit_usd > 0 && s.limits.credit_set_at > 0);
  const when = $derived(entered ? new Date(s.limits.credit_set_at).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }) : '');

  function save() {
    if (!valid) return;
    s.setLimits({ credit_usd: Math.round(amount * 100) / 100, credit_set_at: Date.now() });
    draft = '';
    void imberCreditsStore.refresh(true);
  }

  function clear() {
    s.setLimits({ credit_usd: 0, credit_set_at: 0 });
    void imberCreditsStore.refresh(true);
  }

  const description = $derived(
    (entered ? `$${s.limits.credit_usd.toFixed(2)}, entered ${when}. ` : '') +
      'The balance shown in the Claude Console (Billing), in dollars. The API does not report it, so the title bar estimates what is left: this, less what is spent after you enter it — the organization’s real spending with an Admin key, only Imber’s without one. Enter it again after a top-up, or now and then to correct the estimate.',
  );
</script>

<FormRow label="Credit balance" {description}>
  <div class="row">
    <span class="cur">$</span>
    <Input bind:value={draft} placeholder={entered ? s.limits.credit_usd.toFixed(2) : '0.00'} ariaLabel="Credit balance in dollars"
           onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') save(); }} />
    <Button variant="primary" size="sm" disabled={!valid} onclick={save}>{entered ? 'Update' : 'Save'}</Button>
    {#if entered}<Button variant="ghost" size="sm" onclick={clear}>Clear</Button>{/if}
  </div>
</FormRow>

<style>
  .row { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .row :global(input) { width: 120px; font-family: var(--font-code); font-variant-numeric: tabular-nums; }
  .cur { color: var(--text-muted); font-family: var(--font-code); }
</style>
