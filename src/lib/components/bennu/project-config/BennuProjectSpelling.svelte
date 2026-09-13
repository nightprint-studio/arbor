<script lang="ts">
  /**
   * Project Configuration › Spelling — whether the check runs here, and the words that are correct
   * *in this codebase*.
   *
   * The two halves belong at different levels and always did: the dictionaries are downloaded once
   * per machine and the words you accept everywhere follow you (Settings › Editor › Spelling), while
   * a domain vocabulary — a legacy abbreviation, a table name, a customer's word for a thing —
   * belongs to one repository and should not follow you to the next.
   *
   * Whether the check runs at all stays here too. Identifiers and comments are worth checking on a
   * codebase somebody is writing prose into and noise on a legacy one full of abbreviations, and
   * answering that for one project says nothing about the next.
   */
  import { SpellCheck } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import BennuDictionaryList from '../settings/BennuDictionaryList.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuSpellStore } from '$lib/stores/bennu/spell.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';

  const root = $derived(projectStore.project?.root ?? null);
  const spellOn = $derived(root ? bennuSpellStore.enabledFor(root) : false);
  const installed = $derived(bennuSpellStore.installed);
  /** Whether the backend has answered at all. Until it has, this page says nothing about
   *  dictionaries rather than offering to download ones that are already there. */
  const known = $derived(bennuSpellStore.statusKnown);

  // Asked again here, because the read at window mount can land before the backend is serving.
  $effect(() => {
    void bennuSpellStore.loadStatus();
  });
</script>

<div class="section-header">
  <h2>Spelling</h2>
  <p>
    Whether declared names and comments are checked here, and the words that are correct in this
    codebase and nowhere else.
  </p>
</div>

<div class="card">
  <div class="card-section-title"><SpellCheck size={12} /> The check</div>
  <FormRow
    label="Spell-check identifiers and comments"
    description="Checks declared names — split by camelCase, snake_case and kebab-case — and comment text. A misspelling shows as a hint with an “Add to dictionary” quick-fix."
  >
    <Toggle
      checked={spellOn}
      disabled={!installed || !root}
      onchange={(v) => { if (root) bennuSpellStore.setEnabled(root, v); }}
      ariaLabel="Spell-check identifiers and comments"
      label={!known ? 'Checking…' : installed ? (spellOn ? 'On' : 'Off') : 'No dictionaries installed'}
    />
  </FormRow>
  {#if known && !installed}
    <div class="dl">
      <span class="set-hint">
        The dictionaries are downloaded once and shared by every project.
      </span>
      <Button
        variant="secondary"
        size="sm"
        onclick={() => { bennuUiStore.closeProjectConfig(); bennuUiStore.openSettings('spelling'); }}
      >
        Open Settings › Spelling
      </Button>
    </div>
  {/if}
</div>

<BennuDictionaryList
  scope="project"
  {root}
  title="Words this project accepts"
  hint="A vocabulary that belongs to this codebase — a domain term, a legacy abbreviation, a table name. Kept in the repository, so everyone who clones it gets the same answer. Words that are correct everywhere belong in Settings › Editor › Spelling instead."
/>

<style>
  .dl {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 0 14px 12px;
  }
</style>
