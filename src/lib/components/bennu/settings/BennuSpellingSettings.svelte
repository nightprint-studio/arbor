<script lang="ts">
  /**
   * Settings › Editor › Spelling — the dictionaries on this machine, and the words *you* accept
   * everywhere.
   *
   * Two levels, like naming: this is the one that follows you. A vocabulary that belongs to one
   * codebase — a domain's terms, a legacy abbreviation — goes in Project Configuration › Spelling
   * instead, so it does not follow you to the next project.
   *
   * Whether the check runs stays per project, and deliberately: identifiers and comments are worth
   * checking on a codebase somebody is writing prose into and noise on a legacy one full of
   * abbreviations. The dictionaries themselves are downloaded once, here.
   */
  import { Download, SpellCheck } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import BennuDictionaryList from './BennuDictionaryList.svelte';
  import { bennuSpellStore } from '$lib/stores/bennu/spell.svelte';

  $effect(() => {
    void bennuSpellStore.loadStatus();
  });

  const installed = $derived(bennuSpellStore.installed);
  /** Whether the backend has answered. Offering a download before it has is how dictionaries that
   *  are already on disk got offered again on every open. */
  const known = $derived(bennuSpellStore.statusKnown);
  /** The backend did not answer (or answered with an error): say so, and offer to ask again. */
  const statusError = $derived(bennuSpellStore.statusError);
  const asking = $derived(bennuSpellStore.statusLoading);
  const languages = $derived(bennuSpellStore.status?.languages ?? []);
</script>

<div class="section-header">
  <h2>Spelling</h2>
  <p>
    Checks declared names — split by camelCase, snake_case and kebab-case — and comment text. Which
    projects it runs on is each project's own answer, in <strong>Project Configuration</strong>.
  </p>
</div>

<div class="card">
  <div class="card-section-title"><SpellCheck size={12} /> Dictionaries</div>
  <div class="dicts">
    <div class="dict-state">
      {#if installed}
        <div class="langs">
          {#each languages as lang (lang)}
            <Badge variant="tone" tone="success" label={lang} />
          {/each}
        </div>
        <span class="set-hint">Hunspell, from the LibreOffice dictionaries. Shared by every project.</span>
      {:else if known}
        <span class="set-empty">No dictionaries installed — nothing is being checked anywhere.</span>
      {:else if statusError}
        <span class="set-empty">
          Could not find out which dictionaries are installed — {statusError}. You can still download them.
        </span>
      {:else}
        <span class="set-empty">Asking the backend which dictionaries are installed…</span>
      {/if}
    </div>
    {#if statusError && !known}
      <Button variant="secondary" size="sm" onclick={() => void bennuSpellStore.loadStatus()} disabled={asking}>
        {asking ? 'Asking…' : 'Retry'}
      </Button>
    {/if}
    <!-- Not gated on the status being known: a download answers with the fresh status itself, so a
         backend that could not report one is no reason to refuse the one thing that fixes it. -->
    <Button
      variant={!installed ? 'primary' : 'secondary'}
      size="sm"
      onclick={() => void bennuSpellStore.download()}
      disabled={bennuSpellStore.downloading}
    >
      {#snippet iconStart()}<Download size={13} />{/snippet}
      {bennuSpellStore.downloading
        ? 'Downloading…'
        : installed
          ? 'Re-download'
          : 'Download English + Italian'}
    </Button>
  </div>
  {#if bennuSpellStore.downloading && bennuSpellStore.progress}
    <p class="set-hint set-mono">{bennuSpellStore.progress}</p>
  {/if}
</div>

<BennuDictionaryList
  scope="global"
  title="Words you accept everywhere"
  hint="Spellings that are correct in any project of yours — a framework's name, a tool, a habit of your own. A word that only makes sense in one codebase belongs to that project instead."
/>

<style>
  .dicts {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 10px 14px 12px;
  }
  .dict-state { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .langs { display: flex; flex-wrap: wrap; gap: 6px; }
</style>
