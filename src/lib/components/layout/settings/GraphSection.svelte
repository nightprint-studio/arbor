<script lang="ts">
  import { graphConfigStore } from '$lib/stores/graph_config.svelte';
  import { cacheStore } from '$lib/stores/cache.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';

  let pageSize = $state(parseInt(localStorage.getItem('arbor:graph-page-size') ?? '500'));
  let showRemoteBranches = $state((localStorage.getItem('arbor:show-remotes') ?? 'true') === 'true');
  let showTags = $state((localStorage.getItem('arbor:show-tags') ?? 'true') === 'true');
  let ticketLinksEnabled = $state((localStorage.getItem('arbor:ticket-links-enabled') ?? 'true') === 'true');

  let saved = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    localStorage.setItem('arbor:graph-page-size', String(pageSize));
    localStorage.setItem('arbor:show-remotes', String(showRemoteBranches));
    localStorage.setItem('arbor:show-tags', String(showTags));
    localStorage.setItem('arbor:ticket-links-enabled', String(ticketLinksEnabled));
  });

  async function onTogglePaginate() {
    const newValue = !graphConfigStore.paginate;
    await graphConfigStore.setPaginate(newValue);
    // Invalidate cached graph data so the next load fetches with the correct limit.
    cacheStore.invalidateAll();
    // Re-run CommitGraph's effect (paginate change already triggers it, but
    // refreshTick ensures the reload picks up the latest state even if the
    // reactive dependency was already tracked as the same value).
    graphStore.refresh();
    saved = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => { saved = false; }, 2000);
  }
</script>

<SectionHeader title="Commit Graph" description="Control how the commit history is loaded and displayed." />

<div class="card">
  <FormRow label="Commits per load" description="Loaded in one batch; more added on scroll">
    <NumberStepper bind:value={pageSize} min={100} max={2000} step={100} ariaLabel="Commits per load" />
  </FormRow>

  <FormRow label="Show remote branches" description="Display remote tracking refs on the graph">
    <Toggle bind:checked={showRemoteBranches} />
  </FormRow>

  <FormRow label="Show tags" description="Display annotated and lightweight tags">
    <Toggle bind:checked={showTags} />
  </FormRow>

  <FormRow label="Ticket link chips" description="Show ticket ID chips on commits (may slightly affect scroll performance)">
    <Toggle bind:checked={ticketLinksEnabled} />
  </FormRow>

  <FormRow label="Lazy-load pagination" description="When off, the entire history is loaded at once — disable only on small repos">
    <Toggle
      checked={graphConfigStore.paginate}
      onchange={onTogglePaginate}
      disabled={!graphConfigStore.ready}
    />
  </FormRow>
</div>

{#if saved}
  <p class="saved-label">Saved</p>
{/if}
