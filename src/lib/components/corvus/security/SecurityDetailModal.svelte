<script lang="ts">
  /**
   * Detail modal opened from a counter card click.
   *
   *   • Header: title + risk score badge (if available) + "Open in provider"
   *   • Body:   the shared findings browser — `All | Critical | High | Medium | Low | Info |
   *             Unknown` tabs over a virtualized list (tabs whose count is zero are disabled, so
   *             the shape is stable whichever severity the user entered through).
   *
   * This file owns only what is Corvus-specific: fetching through `securityStore`, the Active /
   * Closed scope switch, and the provider link. Findings are the store's current filtered list —
   * narrowing in the panel narrows the modal's tab counts and rows automatically.
   */
  import { ShieldAlert, ExternalLink } from 'lucide-svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';

  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import SecurityFindingsBrowser from '$lib/components/shared/internal/security/SecurityFindingsBrowser.svelte';

  import { securityStore } from '$lib/stores/corvus/security.svelte';
  import type { Severity, SecuritySummary, SeverityCounts } from '$lib/types/corvus/security';

  interface Props {
    tabId:           string;
    summary:         SecuritySummary;
    initialSeverity: Severity;
    onClose:         () => void;
  }

  let { tabId, summary, initialSeverity, onClose }: Props = $props();

  // Use the store's findings cache. The modal triggers a refresh once on open if the cache is
  // empty for this tab; the panel's Refresh invalidates it.
  $effect(() => {
    const id = tabId;
    if (!id) return;
    if (securityStore.findings.length === 0
        && !securityStore.loading
        && !securityStore.findingsLoading) {
      securityStore.loadFindings(id);
    }
  });

  const loading = $derived(securityStore.loading || securityStore.findingsLoading);
  const error   = $derived(securityStore.error);
  const all     = $derived(securityStore.findings);

  // Tab counts are scope-aware so they always describe the rows shown. In the active scope this
  // matches the panel grid (filteredCounts is active-only by design); in the closed scope we count
  // whatever closed findings the backend returned.
  const scope  = $derived(securityStore.stateScope);
  const counts = $derived.by<SeverityCounts>(() => {
    if (scope === 'active') return securityStore.filteredCounts();
    const out: SeverityCounts = { critical: 0, high: 0, medium: 0, low: 0, info: 0, unknown: 0 };
    for (const f of all) {
      if (f.state === 'detected' || f.state === 'confirmed') continue;
      out[f.severity]++;
    }
    return out;
  });

  async function setScope(next: 'active' | 'closed') {
    if (next === scope) return;
    securityStore.setStateScope(next);
    // Refetch with the new state filter; backend round-trips are cheap.
    if (tabId) await securityStore.loadFindings(tabId);
  }

  const providerLabel = $derived(
    summary.provider_kind === 'gitlab' ? 'GitLab'
      : summary.provider_kind === 'github' ? 'GitHub'
      : 'Provider',
  );

  function openExternal() {
    if (summary.web_url) openUrl(summary.web_url).catch(() => {});
  }
</script>

<Modal {onClose} width="min(94vw, 1080px)" height="82vh" padBody={false} ariaLabel="Security findings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <ShieldAlert size={14} />
      <span class="modal-title">Security findings</span>

      {#if summary.risk_score}
        {@const rs = summary.risk_score}
        <span
          class="risk-pill"
          use:tooltip={`Risk score: ${rs.value.toFixed(1)} (${rs.label})`}
        >{rs.label}</span>
      {/if}

      {#snippet actions()}
        {#if summary.web_url}
          <button class="hdr-btn" onclick={openExternal} use:tooltip={`Open in ${providerLabel}`}>
            <ExternalLink size={13} />
            Open in {providerLabel}
          </button>
        {/if}
      {/snippet}
    </ModalHeader>
  {/snippet}

  <SecurityFindingsBrowser
    findings={all}
    {counts}
    {loading}
    {error}
    initialTab={initialSeverity}
  >
    {#snippet toolbar()}
      <!-- The dashboard panel always shows ACTIVE findings; this lets the modal temporarily flip
           to "Closed" (Resolved / Dismissed) without affecting the panel grid. -->
      <div class="scope-seg" role="radiogroup" aria-label="Finding state scope">
        <button
          type="button"
          class="scope-btn"
          class:scope-active={scope === 'active'}
          aria-pressed={scope === 'active'}
          onclick={() => setScope('active')}
          use:tooltip={'Show open findings (Detected + Confirmed)'}
        >Active</button>
        <button
          type="button"
          class="scope-btn"
          class:scope-active={scope === 'closed'}
          aria-pressed={scope === 'closed'}
          onclick={() => setScope('closed')}
          use:tooltip={'Show managed findings (Resolved + Dismissed)'}
        >Closed</button>
      </div>
    {/snippet}
    {#snippet note()}
      {#if summary.truncated}
        <span class="trunc">Capped at {summary.findings_seen} — refine filters to narrow.</span>
      {/if}
    {/snippet}
  </SecurityFindingsBrowser>
</Modal>

<style>
  .scope-seg {
    display: inline-flex;
    align-items: stretch;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
    flex-shrink: 0;
    margin: 6px 0;
  }
  .scope-btn {
    padding: 4px 10px;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-size: var(--font-size-xs);
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .scope-btn + .scope-btn { border-left: 1px solid var(--border); }
  .scope-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .scope-active {
    background: var(--accent-subtle);
    color: var(--accent);
    font-weight: 600;
  }

  .trunc { color: var(--warning); }

  .risk-pill {
    margin-left: 4px;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--severity-high) 14%, transparent);
    color: var(--severity-high);
    border: 1px solid color-mix(in srgb, var(--severity-high) 32%, transparent);
    font-size: var(--font-size-2xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    font-family: var(--font-ui-sans);
  }

  .hdr-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: var(--font-size-xs);
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .hdr-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
