<script lang="ts">
  /**
   * Saved and recent patterns. Picking one hands it back to the tester, which loads pattern, flags,
   * flavour and the text it was tried on. Saving keeps the current state under a name; the lists
   * themselves live in the Bennu config (see `regex.svelte.ts`).
   */
  import { Trash2, Save } from 'lucide-svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { bennuRegexStore, type KeptPattern } from '$lib/stores/bennu/regex.svelte';

  let { current, onPick }: {
    /** The tester's present state — what Save would keep. */
    current: KeptPattern;
    onPick: (p: KeptPattern) => void;
  } = $props();

  let name = $state('');
  const canSave = $derived(name.trim() !== '' && current.pattern !== '');

  function save() {
    if (!canSave) return;
    bennuRegexStore.save({ ...current, name: name.trim() });
    name = '';
  }
</script>

<div class="lib-save">
  <Input bind:value={name} placeholder="Name this pattern…" size="sm" ariaLabel="Name for the saved pattern"
    onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); save(); } }} />
  <Button variant="secondary" size="sm" disabled={!canSave} onclick={save}>
    <Save size={13} /> Save
  </Button>
</div>

<SectionHeader title="Saved" />
{#if bennuRegexStore.saved.length === 0}
  <EmptyState message="No saved patterns." />
{:else}
  <ul class="lib-list">
    {#each bennuRegexStore.saved as s (s.name)}
      <li class="lib-row">
        <button type="button" class="lib-pick" onclick={() => onPick(s)}>
          <span class="lib-name">{s.name}</span>
          <code class="lib-pattern">{s.pattern}</code>
        </button>
        <IconButton tooltip="Delete" variant="danger" onclick={() => bennuRegexStore.remove(s.name)}>
          <Trash2 size={13} />
        </IconButton>
      </li>
    {/each}
  </ul>
{/if}

<SectionHeader title="Recent">
  {#snippet actions()}
    {#if bennuRegexStore.recent.length > 0}
      <Button variant="ghost" size="xs" onclick={() => bennuRegexStore.clearRecent()}>Clear</Button>
    {/if}
  {/snippet}
</SectionHeader>
{#if bennuRegexStore.recent.length === 0}
  <EmptyState message="Patterns you try show up here." />
{:else}
  <ul class="lib-list">
    {#each bennuRegexStore.recent as r, i (i)}
      <li class="lib-row">
        <button type="button" class="lib-pick" onclick={() => onPick(r)}>
          <code class="lib-pattern">{r.pattern}</code>
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .lib-save { display: flex; gap: 8px; align-items: center; margin-bottom: 8px; }
  .lib-list { list-style: none; margin: 0 0 12px; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .lib-row { display: flex; align-items: center; gap: 4px; }
  .lib-pick {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    padding: 5px 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .lib-pick:hover, .lib-pick:focus-visible { background: var(--bg-hover); outline: none; }
  .lib-name { font-size: var(--font-size-sm); color: var(--text-primary); font-weight: 600; }
  .lib-pattern {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
