<script lang="ts">
  import { onMount } from 'svelte';
  import { ArrowUp, ArrowDown } from 'lucide-svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import ProviderConnectionCard from '$lib/components/shared/internal/ProviderConnectionCard.svelte';
  import { issueProviders } from '$lib/ipc/corvus/providers';
  import type { ProviderDescriptor } from '$lib/types/corvus/providers';
  import { issuesStore } from '$lib/stores/corvus/issues.svelte';
  import type { IssueSortField } from '$lib/types/corvus/issues';
  import { SORT_FIELD_LABELS } from '$lib/types/corvus/issues';

  // The connect UI is fully generic: fetch the self-describing descriptors and
  // render one card per provider. No per-provider code, no bindings.
  let descriptors = $state<ProviderDescriptor[]>([]);

  onMount(async () => {
    try { descriptors = await issueProviders.list(); } catch { descriptors = []; }
  });

  // Keep the Issues sidebar in sync when the user connects/disconnects the
  // provider it's currently showing.
  function syncIfActive(id: string) {
    if (issuesStore.activeProvider === id) void issuesStore.loadAuthStatus();
  }
</script>

<SectionHeader title="Issue Trackers" description="Connect to project management tools. Tokens are stored in the OS keychain." />

{#each descriptors as d (d.id)}
  <div class="provider-slot">
    <ProviderConnectionCard descriptor={d} service={issueProviders} onchange={() => syncIfActive(d.id)} />
  </div>
{/each}

<!-- ── Display Preferences ── -->
<div class="card" style="margin-top:16px">
  <div class="card-section-title">Display Preferences</div>
  <div class="card-row-note">
    Default sort order applied to the Issues sidebar and Ticket Picker. Changes are saved immediately.
  </div>

  <FormRow label="Sort by" description="Field used to order issues">
    <Select
      value={issuesStore.sortField}
      options={Object.entries(SORT_FIELD_LABELS).map(([field, label]) => ({ value: field, label }))}
      onchange={(v) => issuesStore.setSort(v as IssueSortField, issuesStore.sortDir)}
    />
  </FormRow>

  <FormRow label="Direction" description="Ascending or descending order">
    <div class="sort-dir-toggle">
      <button
        class="dir-btn"
        class:dir-btn-active={issuesStore.sortDir === 'asc'}
        onclick={() => issuesStore.setSort(issuesStore.sortField, 'asc')}
        use:tooltip={'Ascending'}
      >
        <ArrowUp size={12} /> Ascending
      </button>
      <button
        class="dir-btn"
        class:dir-btn-active={issuesStore.sortDir === 'desc'}
        onclick={() => issuesStore.setSort(issuesStore.sortField, 'desc')}
        use:tooltip={'Descending'}
      >
        <ArrowDown size={12} /> Descending
      </button>
    </div>
  </FormRow>
</div>

<style>
  .provider-slot { margin-bottom: 12px; }

  /* Sort direction toggle */
  .sort-dir-toggle { display: flex; gap: 4px; }
  .dir-btn {
    display: flex; align-items: center; gap: 5px;
    padding: 4px 10px;
    font-size: 11px;
    font-family: var(--font-ui-sans);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .dir-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .dir-btn-active {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
