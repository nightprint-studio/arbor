<script lang="ts">
  import { onMount } from 'svelte';
  import { ArrowUp, ArrowDown } from 'lucide-svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import IssueProviderCard, { type ProviderBinding } from '$lib/components/shared/internal/IssueProviderCard.svelte';
  import { issuesStore } from '$lib/stores/issues.svelte';
  import {
    listIssueProviders, linearGetAuthStatus, linearSaveToken, linearLogout,
    jiraGetAuthStatus, jiraSaveBasicAuth,
  } from '$lib/ipc/issues';
  import { startLinearOAuth, disconnectLinearOAuth, startJiraOAuth, disconnectJira } from '$lib/ipc/auth';
  import type { IssueSortField, IssueSortDir, ProviderDescriptor } from '$lib/types/issues';
  import { SORT_FIELD_LABELS } from '$lib/types/issues';

  let descriptors = $state<ProviderDescriptor[]>([]);

  onMount(async () => {
    try { descriptors = await listIssueProviders(); } catch { descriptors = []; }
  });

  // ── Per-provider glue (the irreducible bits the descriptor can't carry) ──────
  // Connect/disconnect/status use the direct IPC; `afterChange` refreshes the
  // shared store only when the sidebar is currently showing that provider.

  function syncIfActive(id: 'linear' | 'jira') {
    if (issuesStore.activeProvider === id) void issuesStore.loadAuthStatus();
  }

  const bindings: Record<string, ProviderBinding> = {
    linear: {
      brandColor:       'var(--brand-linear)',
      oauthEvent:       'arbor://linear-oauth-done',
      startOAuth:       () => startLinearOAuth(),
      connectFields:    async (v) => { await linearSaveToken(v.token); },
      disconnect:       async () => { await linearLogout(); await disconnectLinearOAuth().catch(() => {}); },
      loadStatus:       async () => {
        const s = await linearGetAuthStatus();
        return {
          authenticated: s.authenticated,
          user: s.user ? { displayName: s.user.displayName, email: s.user.email, avatarUrl: s.user.avatarUrl } : null,
          domain: null,
        };
      },
      afterChange:      () => syncIfActive('linear'),
      oauthHintIdle:    'Opens Linear in the browser to authorize Arbor.',
      oauthHintWaiting: 'Browser opened — approve access in Linear then return here.',
      oauthIdleLabel:   'Authorize with Linear',
      advancedProvider: 'linear',
    },
    jira: {
      brandColor:       'var(--brand-jira)',
      oauthEvent:       'arbor://jira-oauth-done',
      startOAuth:       () => startJiraOAuth(),
      connectFields:    async (v) => { await jiraSaveBasicAuth(v.email ?? '', v.api_token, v.domain); },
      disconnect:       async () => { await disconnectJira(); },
      loadStatus:       async () => {
        const s = await jiraGetAuthStatus();
        return {
          authenticated: s.authenticated,
          user: s.user ? { displayName: s.user.displayName, email: s.user.email, avatarUrl: s.user.avatarUrl } : null,
          domain: s.domain,
        };
      },
      afterChange:      () => syncIfActive('jira'),
      validateFields:   (v) => {
        const isCloud = (v.domain ?? '').trim().endsWith('.atlassian.net');
        return !(isCloud && !(v.email ?? '').trim());
      },
      fieldsHint:       (v) => {
        const isCloud = (v.domain ?? '').trim().endsWith('.atlassian.net');
        return isCloud
          ? 'Jira Cloud: email + API token from id.atlassian.com → Security → API tokens.'
          : 'Jira Data Center/Server: Personal Access Token from Jira → Profile → Personal Access Tokens.';
      },
      oauthHintIdle:    'Opens Atlassian in the browser to authorize Arbor. Requires a configured Atlassian OAuth app.',
      oauthHintWaiting: 'Browser opened — approve access in Atlassian then return here.',
      oauthIdleLabel:   'Authorize with Atlassian',
      advancedProvider: 'jira',
    },
  };
</script>

<SectionHeader title="Issue Trackers" description="Connect to project management tools. Tokens are stored in the OS keychain." />

{#each descriptors as d (d.id)}
  {#if bindings[d.id]}
    <div class="provider-slot">
      <IssueProviderCard descriptor={d} binding={bindings[d.id]} />
    </div>
  {/if}
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
