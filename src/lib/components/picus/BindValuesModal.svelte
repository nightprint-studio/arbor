<script lang="ts">
  /**
   * The values a parameterised statement is waiting for, asked once, before
   * anything is sent.
   *
   * ## Why this exists at all
   *
   * A placeholder is the alternative to editing the literal in the buffer every
   * time. That is worth a dialog because of what it *prevents*: a value typed into
   * a box is bound, so it reaches the server in the protocol's own field and can
   * never be read as syntax — no quoting, no doubled apostrophe in a surname, no
   * statement that means something other than what it looks like.
   *
   * ## NULL is a switch, not an empty box
   *
   * `''` and `NULL` are different rows on a text column, and confusing them is
   * exactly how a wrong `UPDATE` gets written. So each value has an explicit NULL
   * toggle and clearing the box means the empty string — never "I suppose they
   * meant nothing".
   *
   * ## Keyboard
   *
   * The first box is focused on open, Tab walks them in the order the statement
   * reads, Esc cancels and both Enter and Ctrl+Enter run. Enter is safe here
   * because every control is single-line: there is nothing it could be inserting.
   */
  import { untrack } from 'svelte';
  import { Variable } from 'lucide-svelte';

  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import {
    emptyEntry,
    picusBindsStore,
    type BindEntry,
    type BindPrompt,
  } from '$lib/stores/picus/binds.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';

  let { prompt, onClose }: { prompt: BindPrompt; onClose: () => void } = $props();

  /**
   * The editable copy, seeded from what this tab last supplied.
   *
   * A copy rather than the store's own entries: cancelling must leave the
   * remembered values exactly as they were, and editing in place would have
   * half-typed values survive a dialog the user backed out of.
   */
  // svelte-ignore state_referenced_locally
  let entries = $state<BindEntry[]>(seed(prompt));

  function seed(p: BindPrompt): BindEntry[] {
    return p.slots.map((slot) => ({ ...picusBindsStore.entry(p.tabId, slot.label) }));
  }

  /**
   * Re-seed when the dialog is handed a different statement.
   *
   * The `{#if}` that renders it keeps one component alive across prompts, so the
   * initialiser above only ever runs for the first. `untrack` around the read:
   * the boxes must follow the *prompt*, and taking a dependency on the remembered
   * values as well would wipe half-typed input the moment anything wrote them.
   */
  $effect(() => {
    const next = prompt;
    entries = untrack(() => seed(next));
  });

  const connection = $derived(connectionsStore.byId(prompt.connectionId));

  /** How many of the boxes are being sent as a real NULL — said in the footer, so
   *  the one thing that is easy to leave switched on by mistake is visible. */
  const nulls = $derived(entries.filter((e) => e.isNull).length);

  function submit() {
    const remembered: Record<string, BindEntry> = {};
    prompt.slots.forEach((slot, i) => {
      remembered[slot.label] = { ...(entries[i] ?? emptyEntry()) };
    });
    picusBindsStore.remember(prompt.tabId, remembered);
    // Closed BEFORE the run: the run resolves the buffer again and would otherwise
    // find the same placeholders and ask a second time.
    picusBindsStore.close();
    void queryStore.run(prompt.tabId, prompt.connectionId, prompt.scope, { bindsResolved: true });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    // Stopped as well as prevented, and only for Enter: the window behind this
    // dialog binds Ctrl+Enter to Run, and letting it through would start a second
    // run that finds the same placeholders and re-opens the dialog that just
    // closed. Every other key — Esc above all — still reaches the shell.
    e.stopPropagation();
    submit();
  }
</script>

<Modal {onClose} width="560px" ariaLabel="Values for this statement">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Variable size={14} />
      <span class="modal-title">Values for this statement</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="bv" role="group" onkeydown={onKeydown}>
    <p class="bv-lead">
      {prompt.slots.length === 1 ? 'This statement takes one value' : `This statement takes ${prompt.slots.length} values`}
      {#if connection}on <strong>{connection.name}</strong>{/if}. They are sent beside the
      statement, so nothing here is quoted, escaped or read as SQL.
    </p>

    <div class="bv-fields">
      {#each prompt.slots as slot, i (slot.label)}
        <FormField label={slot.label} for={`bind-${i}`}>
          <div class="bv-row">
            <Input
              id={`bind-${i}`}
              bind:value={entries[i].text}
              disabled={entries[i].isNull}
              autofocus={i === 0}
              size="sm"
              placeholder={entries[i].isNull ? 'NULL' : 'value'}
              ariaLabel={`Value for ${slot.label}`}
            />
            <Toggle
              bind:checked={entries[i].isNull}
              size="sm"
              label="NULL"
              ariaLabel={`Send ${slot.label} as NULL`}
            />
          </div>
        </FormField>
      {/each}
    </div>

    <p class="bv-note">
      An empty box is the empty string. NULL is the switch — on a text column the two
      match different rows.
    </p>
  </div>

  {#snippet footer()}
    <span class="bv-foot">
      {#if nulls}
        {nulls === 1 ? 'One value is NULL' : `${nulls} values are NULL`}.
      {:else}
        Remembered for this tab until the window closes.
      {/if}
    </span>
    <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
    <Button
      variant="primary"
      size="sm"
      tooltip={{ content: 'Run with these values', shortcut: 'Ctrl+Enter' }}
      onclick={submit}
    >
      Run
    </Button>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }

  .bv { display: flex; flex-direction: column; gap: 12px; }

  .bv-lead { margin: 0; font-size: var(--font-size-sm); line-height: 1.55; color: var(--text-primary); }
  .bv-lead strong { font-weight: 600; }

  /* Bounded rather than growing without limit: a statement with a dozen
     placeholders must not push the footer off screen. */
  .bv-fields {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-height: 320px;
    overflow-y: auto;
  }

  .bv-row { display: flex; align-items: center; gap: 10px; }

  .bv-note { margin: 0; font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); }

  .bv-foot { flex: 1; font-size: var(--font-size-xs); color: var(--text-muted); text-align: left; }
</style>
