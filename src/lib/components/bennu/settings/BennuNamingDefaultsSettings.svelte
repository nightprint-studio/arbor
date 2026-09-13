<script lang="ts">
  /**
   * Settings › Editor › Naming — how *you* spell declarations, in every project.
   *
   * The level above a project's own `[naming]` section. A project states what it spells differently
   * and takes the rest from here, so adopting a convention is done once rather than in every
   * checkout — and changing your mind is one edit rather than a tour of them.
   *
   * The grid itself is `BennuNamingSettings`, handed the profile document. It is the same screen
   * Project Configuration shows, with nothing above it to inherit from.
   */
  import { untrack } from 'svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import BennuNamingSettings from '../BennuNamingSettings.svelte';
  import { bennuNamingStore } from '$lib/stores/bennu/naming.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';

  const doc = $derived(bennuNamingStore.profile);

  $effect(() => {
    const root = projectStore.project?.root ?? null;
    // Untracked: both read the state they then write, and depending on that would cost a
    // round-trip per render for nothing. The project root is the only real dependency — it decides
    // which language columns the catalog marks as present.
    untrack(() => {
      void bennuNamingStore.loadCatalog(root);
      void bennuNamingStore.profile.load();
    });
  });

  async function save() {
    if (!(await bennuNamingStore.profile.apply())) {
      toastStore.show("Couldn't save the naming conventions", 'error');
    }
  }
</script>

<div class="section-header">
  <h2>Naming</h2>
  <p>
    How you spell declarations, in every project. A project takes these unless it says otherwise —
    <strong>Project Configuration › Naming</strong> is where it does.
  </p>
</div>

<BennuNamingSettings {doc} />

<div class="actions">
  <Button variant="ghost" size="sm" onclick={() => doc.revert()} disabled={!doc.dirty}>
    Discard changes
  </Button>
  <Button variant="primary" size="sm" onclick={save} disabled={!doc.dirty || doc.saving}>
    {doc.saving ? 'Saving…' : 'Save defaults'}
  </Button>
</div>

<style>
  /* This page owns an Apply, unlike the rest of Settings: the grid is a document, not a row of
     switches, and writing it on every keystroke would have the backend re-reading the conventions
     of every open project while a convention is still being chosen. */
  .actions { display: flex; align-items: center; justify-content: flex-end; gap: 8px; }
</style>
