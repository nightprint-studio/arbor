<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Network, ArrowRight, AlertTriangle, CheckCircle2, XCircle,
    RefreshCw, ChevronRight,
  } from 'lucide-svelte';
  import { pluginDepGraph, type DepGraphNode } from '$lib/ipc/plugin';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let { onClose }: { onClose: () => void } = $props();

  let graph   = $state<DepGraphNode[]>([]);
  let loading = $state(true);
  let error   = $state<string | null>(null);
  let selected = $state<string | null>(null);

  async function load() {
    loading = true;
    error   = null;
    try {
      graph = await pluginDepGraph();
      if (graph.length > 0 && !selected) selected = graph[0].name;
    } catch (e) {
      error = `${e}`;
    } finally {
      loading = false;
    }
  }

  onMount(load);

  const selectedNode = $derived.by<DepGraphNode | null>(() => {
    if (!selected) return null;
    return graph.find(n => n.name === selected) ?? null;
  });

  /** Count of unmet required edges across the whole graph. */
  const issueCount = $derived.by(() => {
    let n = 0;
    for (const node of graph) {
      for (const e of node.depends_on) if (e.unmet && !e.optional) n++;
    }
    return n;
  });
</script>

<Modal {onClose} width="min(760px, 92vw)" height="min(560px, 85vh)" padBody={false} ariaLabel="Plugin dependency graph">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Network size={13} />
      <span class="modal-title">Plugin Dependency Graph</span>
      {#snippet actions()}
        {#if issueCount > 0}
          <span class="dg-badge dg-badge-warn" use:tooltip={`${issueCount} unmet required dependency(ies)`}>
            <AlertTriangle size={10} /> {issueCount} unmet
          </span>
        {:else if graph.length > 0}
          <span class="dg-badge dg-badge-ok" use:tooltip={'All dependencies resolved'}>
            <CheckCircle2 size={10} /> All resolved
          </span>
        {/if}
        <button class="dg-icon-btn" onclick={load} use:tooltip={'Refresh'} disabled={loading}>
          <RefreshCw size={12} class={loading ? 'spinning' : ''} />
        </button>
      {/snippet}
    </ModalHeader>
  {/snippet}

  <!-- Body -->
  <div class="dg-body">
    {#if loading}
      <div class="dg-empty">
        <RefreshCw size={18} class="spinning" />
        <span style="margin-left: 8px">Loading dependency graph…</span>
      </div>
    {:else if error}
      <div class="dg-error"><XCircle size={14} /> {error}</div>
    {:else if graph.length === 0}
      <div class="dg-empty">No plugins loaded.</div>
    {:else}
      <!-- Left: plugin nav (Settings-style) -->
      <nav class="dg-nav" aria-label="Plugins">
        <div class="dg-nav-group-label">Plugins · {graph.length}</div>
        <div class="dg-nav-list">
          {#each graph as node (node.name)}
            {@const depErrors = node.depends_on.filter(e => e.unmet && !e.optional).length}
            <button
              class="dg-nav-item"
              class:active={selected === node.name}
              class:disabled={!node.enabled}
              onclick={() => { selected = node.name; }}
              use:tooltip={node.enabled ? '' : 'Plugin is disabled'}
            >
              <span class="dg-nav-name">{node.name}</span>
              <span class="dg-nav-version">v{node.version}</span>
              {#if node.error}
                <AlertTriangle size={11} class="dg-nav-err" />
              {:else if depErrors > 0}
                <span class="dg-nav-err-count">{depErrors}</span>
              {/if}
              {#if selected === node.name}
                <ChevronRight size={11} class="dg-nav-arrow" />
              {/if}
            </button>
          {/each}
        </div>
      </nav>

      <!-- Right: selected plugin details -->
      <div class="dg-content">
        {#if selectedNode}
          {@const node = selectedNode}

          <!-- Plugin header block -->
          <header class="dg-plugin-head">
            <div class="dg-plugin-titles">
              <h2>
                {node.name}
                <span class="dg-plugin-ver">v{node.version}</span>
              </h2>
              <p>
                {node.enabled ? 'Active plugin.' : 'This plugin is currently disabled.'}
              </p>
            </div>
            {#if !node.enabled}
              <span class="dg-muted-tag">Disabled</span>
            {/if}
          </header>

          {#if node.error}
            <div class="dg-box dg-box-err">
              <AlertTriangle size={12} />
              <span>{node.error}</span>
            </div>
          {/if}

          <!-- Depends on card -->
          <section class="dg-card">
            <header class="dg-card-title">
              <ArrowRight size={11} />
              Depends on
              {#if node.depends_on.length > 0}
                <span class="dg-card-count">{node.depends_on.length}</span>
              {/if}
            </header>
            {#if node.depends_on.length === 0}
              <div class="dg-card-empty">No declared dependencies.</div>
            {:else}
              {#each node.depends_on as e (e.name)}
                {@const resolvable = !!graph.find(g => g.name === e.name)}
                <div class="dg-card-row"
                     class:unmet={e.unmet && !e.optional}
                     class:optional={e.optional}>
                  <div class="dg-row-label">
                    <button
                      class="dg-row-link"
                      onclick={() => { if (resolvable) selected = e.name; }}
                      disabled={!resolvable}
                    >{e.name}</button>
                    {#if e.version}
                      <span class="dg-row-desc">Requires {e.version}</span>
                    {/if}
                  </div>
                  <div class="dg-row-tags">
                    {#if e.optional}<span class="dg-tag-soft">optional</span>{/if}
                    {#if e.unmet && !e.optional}<span class="dg-tag-err">unmet</span>
                    {:else if e.unmet && e.optional}<span class="dg-tag-warn">unmet</span>
                    {:else if resolvable}<span class="dg-tag-ok">resolved</span>{/if}
                  </div>
                </div>
              {/each}
            {/if}
          </section>

          <!-- Required by card -->
          <section class="dg-card">
            <header class="dg-card-title">
              <ArrowRight size={11} class="dg-arrow-rev" />
              Required by
              {#if node.dependents.length > 0}
                <span class="dg-card-count">{node.dependents.length}</span>
              {/if}
            </header>
            {#if node.dependents.length === 0}
              <div class="dg-card-empty">No other plugin depends on this one.</div>
            {:else}
              {#each node.dependents as e (e.name)}
                <div class="dg-card-row" class:optional={e.optional}>
                  <div class="dg-row-label">
                    <button class="dg-row-link" onclick={() => { selected = e.name; }}>{e.name}</button>
                    {#if e.version}
                      <span class="dg-row-desc">Requires v{e.version}</span>
                    {/if}
                  </div>
                  <div class="dg-row-tags">
                    {#if e.optional}<span class="dg-tag-soft">optional</span>{/if}
                  </div>
                </div>
              {/each}
            {/if}
          </section>
        {/if}
      </div>
    {/if}
  </div>
</Modal>

<style>
  .dg-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 999px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }
  .dg-badge-ok {
    background: color-mix(in srgb, var(--success) 16%, transparent);
    color: var(--success);
    border: 1px solid color-mix(in srgb, var(--success) 35%, transparent);
  }
  .dg-badge-warn {
    background: color-mix(in srgb, var(--error) 14%, transparent);
    color: var(--error);
    border: 1px solid color-mix(in srgb, var(--error) 35%, transparent);
  }

  .dg-icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .dg-icon-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .dg-icon-btn:disabled { opacity: 0.45; cursor: default; }

  /* ── Body / two-pane layout (mirrors Settings) ────────────────────────── */

  .dg-body {
    display: flex;
    height: 100%;
    min-height: 0;
    background: var(--bg-elevated);
    padding: 4px;
    gap: 4px;
  }
  .dg-empty, .dg-error {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    padding: 30px;
  }
  .dg-error { color: var(--error); gap: 8px; }

  /* ── Nav (mirrors Settings .nav) ──────────────────────────────────────── */

  .dg-nav {
    width: 180px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }

  .dg-nav-group-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 10px 14px 8px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .dg-nav-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    overflow-y: auto;
    flex: 1;
  }

  .dg-nav-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast), box-shadow var(--transition-fast);
    position: relative;
  }
  .dg-nav-item:hover:not(.active) {
    background: var(--bg-overlay);
    color: var(--text-primary);
    border-color: var(--border);
    box-shadow: 0 1px 4px rgba(0,0,0,0.15);
  }
  .dg-nav-item.active {
    background: var(--accent-subtle);
    color: var(--accent);
    font-weight: 500;
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .dg-nav-item.disabled:not(.active) .dg-nav-name { color: var(--text-muted); }

  .dg-nav-name    { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dg-nav-version {
    font-size: 10px;
    color: var(--text-disabled);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }
  .dg-nav-item.active .dg-nav-version { color: color-mix(in srgb, var(--accent) 60%, var(--text-muted)); }
  :global(.dg-nav-arrow) { opacity: 0.55; flex-shrink: 0; }
  :global(.dg-nav-err) { color: var(--error); flex-shrink: 0; }
  .dg-nav-err-count {
    font-size: 9px;
    color: var(--text-on-accent);
    background: var(--error);
    border-radius: 999px;
    padding: 0 5px;
    font-weight: 600;
  }

  /* ── Content area (mirrors Settings .content) ─────────────────────────── */

  .dg-content {
    flex: 1;
    min-height: 0;
    padding: 22px 24px 28px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }

  /* Plugin header — mirrors Settings section-header */
  .dg-plugin-head {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin-bottom: 4px;
  }
  .dg-plugin-titles { flex: 1; display: flex; flex-direction: column; gap: 2px; }
  .dg-plugin-head h2 {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .dg-plugin-ver {
    font-size: 11px;
    color: var(--text-muted);
    font-weight: 400;
    font-variant-numeric: tabular-nums;
  }
  .dg-plugin-head p {
    font-size: 11px;
    color: var(--text-muted);
    margin: 0;
    line-height: 1.5;
  }

  .dg-muted-tag {
    font-size: 9.5px;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--bg-base);
    color: var(--text-muted);
    border: 1px solid var(--border-subtle);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    font-weight: 600;
    flex-shrink: 0;
  }

  /* Plugin-level error box */
  .dg-box {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    line-height: 1.4;
  }
  .dg-box-err {
    background: color-mix(in srgb, var(--error) 12%, transparent);
    color: var(--error);
    border: 1px solid color-mix(in srgb, var(--error) 35%, transparent);
  }

  /* ── Cards (mirrors Settings .card) ───────────────────────────────────── */

  .dg-card {
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .dg-card-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 10px 14px 8px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
  }
  :global(.dg-arrow-rev) { transform: rotate(180deg); }
  .dg-card-count {
    margin-left: auto;
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    padding: 1px 7px;
    font-variant-numeric: tabular-nums;
  }

  .dg-card-empty {
    padding: 12px 14px;
    font-size: 11.5px;
    color: var(--text-muted);
    font-style: italic;
  }

  /* Rows inside a card — mirrors .card-row from Settings */
  .dg-card-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border-subtle);
    transition: background var(--transition-fast);
  }
  .dg-card-row:last-child { border-bottom: none; }
  .dg-card-row:hover      { background: rgba(255, 255, 255, 0.015); }
  .dg-card-row.unmet      {
    background: color-mix(in srgb, var(--error) 7%, transparent);
    border-left: 2px solid color-mix(in srgb, var(--error) 55%, transparent);
    padding-left: 12px;
  }
  .dg-card-row.optional   { opacity: 0.82; }

  .dg-row-label {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .dg-row-link {
    background: transparent;
    border: none;
    color: var(--accent);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    padding: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dg-row-link:hover { color: var(--accent-hover); text-decoration: underline; text-underline-offset: 2px; }
  .dg-row-link:disabled {
    color: var(--text-primary);
    cursor: default;
    font-weight: 450;
  }
  .dg-row-desc {
    font-size: 10.5px;
    color: var(--text-muted);
    font-family: var(--font-code);
    line-height: 1.3;
  }

  .dg-row-tags {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  /* ── Tags ─────────────────────────────────────────────────────────────── */

  .dg-tag-soft, .dg-tag-warn, .dg-tag-err, .dg-tag-ok {
    font-size: 9px;
    padding: 2px 6px;
    border-radius: 999px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    font-weight: 600;
    white-space: nowrap;
  }
  .dg-tag-soft {
    background: var(--bg-elevated);
    color: var(--text-muted);
    border: 1px solid var(--border-subtle);
  }
  .dg-tag-warn {
    background: color-mix(in srgb, var(--warning) 14%, transparent);
    color: var(--warning);
    border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
  }
  .dg-tag-err {
    background: color-mix(in srgb, var(--error) 14%, transparent);
    color: var(--error);
    border: 1px solid color-mix(in srgb, var(--error) 35%, transparent);
  }
  .dg-tag-ok {
    background: color-mix(in srgb, var(--success) 14%, transparent);
    color: var(--success);
    border: 1px solid color-mix(in srgb, var(--success) 30%, transparent);
  }

  /* ── Animations ───────────────────────────────────────────────────────── */

  :global(.spinning) { animation: dg-spin 1s linear infinite; }
  @keyframes dg-spin { to { transform: rotate(360deg); } }
</style>
