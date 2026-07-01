<script lang="ts">
  import { ChevronDown } from 'lucide-svelte';
  import { slide } from 'svelte/transition';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';

  const SQUASH_HINTS_KEY = 'arbor:experimental:squash-merge-hints';

  let squashMergeHints = $state(
    // Default OFF: even with the new background-loaded hints (graph renders
    // before the API call returns) the network round-trip can take many
    // seconds on slow links / rate-limited tokens.  Opt-in only.
    (localStorage.getItem(SQUASH_HINTS_KEY) ?? 'false') === 'true'
  );
  let squashExpanded = $state(false);

  $effect(() => {
    localStorage.setItem(SQUASH_HINTS_KEY, String(squashMergeHints));
  });
</script>

<SectionHeader title="Experimental" description="Features in active testing that may rely on external services or behave unexpectedly in edge cases." />

<div class="card">
  <!-- Toggle row: title + toggle inline, desc below, collapsible detail -->
  <div class="feat-row">
    <div class="feat-header">
      <div class="feat-title-group">
        <span class="feat-title">Squash-merge ghost edges</span>
        <span class="feat-badge">API</span>
      </div>
      <Toggle bind:checked={squashMergeHints} />
    </div>

    <p class="feat-summary">
      Queries GitHub / GitLab for merged PRs and draws a dashed edge in the graph connecting
      each squash commit to its source branch tip — making squash merges visually traceable.
    </p>

    {#if squashMergeHints}
      <div class="feat-warning">
        <Alert variant="warning" compact noIcon
          text="Sconsigliato per motivi di performance: ogni caricamento del grafo aggiunge una chiamata API a GitHub/GitLab che, su connessioni lente o quote provider sature, può aggiungere diversi secondi (osservati fino a 10s). La chiamata adesso è non-bloccante — il grafo appare subito e le ghost edge compaiono dopo — ma resta consigliato disattivare se non ti servono visivamente." />
      </div>
    {/if}

    <button
      class="expand-btn"
      onclick={() => { squashExpanded = !squashExpanded; }}
      aria-expanded={squashExpanded}
    >
      <ChevronDown size={13} class="chevron {squashExpanded ? 'open' : ''}" />
      {squashExpanded ? 'Hide details' : 'How it works'}
    </button>

    {#if squashExpanded}
      <div class="feat-detail" transition:slide={{ duration: 160 }}>
        <p>
          When a PR/MR is squash-merged, git creates a single new commit on the target branch
          with no topological link to the original feature commits. Arbor retrieves the
          <code>merge_commit_sha</code> from the API and uses it to draw a dashed ghost edge
          between the merge point and the feature branch tip.
        </p>
        <ul>
          <li>If the merge commit isn't local yet, the edge anchors to the pre-merge target tip as a fallback.</li>
          <li>Once you run <code>git fetch</code>, native edges replace the ghost automatically.</li>
          <li>Requires a GitHub or GitLab token (<em>Settings → Access → Git & Integrations</em>). Silently disabled if no token is found.</li>
          <li><strong>Performance:</strong> aggiunge una chiamata API per ogni caricamento del grafo (fino a 50 PR chiuse). Adesso è caricata in background (non blocca il render del grafo), ma rete lenta o rate-limit del provider possono comunque ritardare la comparsa delle ghost edge di diversi secondi. Se non ti servono visivamente, lascia disabilitata.</li>
        </ul>
      </div>
    {/if}
  </div>
</div>

<style>
  /* Feature row */
  .feat-row {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .feat-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .feat-title-group {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .feat-title {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .feat-badge {
    font-size: 0.65rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    background: var(--accent-subtle);
    color: var(--accent);
    text-transform: uppercase;
  }

  .feat-summary {
    font-size: 0.77rem;
    color: var(--text-secondary);
    line-height: 1.55;
    margin: 0;
  }

  .feat-warning {
    margin-top: 4px;
  }

  /* Expand button */
  .expand-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    padding: 0;
    font-size: 0.73rem;
    color: var(--accent);
    cursor: pointer;
    width: fit-content;
    opacity: 0.85;
    transition: opacity var(--anim-dur-base);
  }
  .expand-btn:hover { opacity: 1; }

  :global(.expand-btn .chevron) {
    transition: transform var(--anim-dur-base);
  }
  :global(.expand-btn .chevron.open) {
    transform: rotate(180deg);
  }

  /* Detail block */
  .feat-detail {
    font-size: 0.76rem;
    color: var(--text-secondary);
    line-height: 1.6;
    border-top: 1px solid var(--border-subtle);
    padding-top: 10px;
    margin-top: 2px;
  }

  .feat-detail p {
    margin: 0 0 8px;
  }

  .feat-detail ul {
    margin: 0;
    padding-left: 16px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .feat-detail code {
    font-family: var(--font-code);
    font-size: 0.71rem;
    background: var(--bg-overlay);
    padding: 0 3px;
    border-radius: var(--radius-sm);
    color: var(--accent);
  }

  .feat-detail em {
    font-style: normal;
    color: var(--text-primary);
  }
</style>
