<script lang="ts">
  import { graphConfigStore } from '$lib/stores/graph_config.svelte';
  import { cacheStore } from '$lib/stores/cache.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';

  const pageSize             = $derived(graphConfigStore.pageSize);
  const showRemoteBranches   = $derived(graphConfigStore.showRemoteBranches);
  const showTags             = $derived(graphConfigStore.showTags);
  const ticketLinksEnabled   = $derived(graphConfigStore.ticketLinksEnabled);

  let saved = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

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
    <NumberStepper
      value={pageSize}
      onchange={(n) => graphConfigStore.setPageSize(n)}
      min={100}
      max={2000}
      step={100}
      ariaLabel="Commits per load"
    />
  </FormRow>

  <FormRow label="Show remote branches" description="Display remote tracking refs on the graph">
    <Toggle checked={showRemoteBranches} onchange={(v) => graphConfigStore.setShowRemoteBranches(v)} />
  </FormRow>

  <FormRow label="Show tags" description="Display annotated and lightweight tags">
    <Toggle checked={showTags} onchange={(v) => graphConfigStore.setShowTags(v)} />
  </FormRow>

  <FormRow label="Ticket link chips" description="Show ticket ID chips on commits (may slightly affect scroll performance)">
    <Toggle checked={ticketLinksEnabled} onchange={(v) => graphConfigStore.setTicketLinksEnabled(v)} />
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
