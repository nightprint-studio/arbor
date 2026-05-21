<script lang="ts">
  /**
   * Dependency-tree modal opened from a tree-kind sidebar's right-click menu.
   *
   * Protocol — kept deliberately small:
   *   1. Modal generates a unique `request_id`.
   *   2. Fires `<provider_action>` on the provider plugin with `{node_id, data,
   *      request_id}`. Fire-and-forget — no return value.
   *   3. The provider does its work (typically: spawn `cargo tree` /
   *      `mvn dependency:tree` / `npm ls --json` via `arbor.job.spawn`, parse
   *      the output) and pushes the result via
   *      `arbor.ui.tree.set(request_id, { title, nodes })`.
   *   4. The modal subscribes to that snapshot via the contribution store and
   *      re-renders as soon as it arrives.
   */
  import { onMount } from 'svelte';
  import { Search, Loader, AlertCircle } from 'lucide-svelte';
  import Tree from './ui/Tree.svelte';
  import PluginIcon from '$lib/components/plugins/PluginIcon.svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { firePluginAction } from '$lib/ipc/plugin';
  import type { TreeNode } from '$lib/types/contribution';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';

  interface Props {
    pluginName:     string;
    providerAction: string;
    /** The node the user right-clicked — passed to the provider so it knows
     *  which module/crate/pom to compute dependencies for. */
    node:           TreeNode;
    title?:         string;
    onClose:        () => void;
  }
  let { pluginName, providerAction, node, title = 'Dependencies', onClose }: Props = $props();

  const requestId = `dep-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  let filter   = $state('');
  let timedOut = $state(false);

  const snapshot = $derived(contributionStore.tree(pluginName, requestId));
  const nodes    = $derived(snapshot?.nodes ?? []);
  const ready    = $derived(snapshot !== null);
  const finalTitle = $derived(snapshot?.title ?? title);

  onMount(() => {
    contributionStore.ensureTree(pluginName, requestId);
    firePluginAction(pluginName, providerAction, JSON.stringify({
      request_id: requestId,
      node_id:    node.id,
      data:       node.data,
    })).catch(() => { timedOut = true; });

    const t = setTimeout(() => { if (!ready) timedOut = true; }, 60_000);
    return () => clearTimeout(t);
  });
</script>

<Modal {onClose} size="lg" padBody={false} ariaLabel={finalTitle}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <div class="header-title">
        <span class="title-text">{finalTitle}</span>
        <span class="title-sub">{node.label}</span>
      </div>
    </ModalHeader>
  {/snippet}

  <div class="modal-content">
    <div class="card-toolbar">
      <span class="search-wrap">
        <Search size={12} class="search-ic" />
        <input
          type="text"
          placeholder="Filter dependencies…"
          bind:value={filter}
        />
        {#if filter}
          <button class="clear" onclick={() => filter = ''}>×</button>
        {/if}
      </span>
    </div>

    <div class="card-body">
      {#if !ready && !timedOut}
        <div class="state state-loading">
          <Loader size={18} class="spin" />
          <span>Resolving dependencies…</span>
        </div>
      {:else if timedOut && !ready}
        <div class="state state-error">
          <AlertCircle size={18} />
          <span>The provider didn't respond within 60s. Check the plugin logs.</span>
        </div>
      {:else if nodes.length === 0}
        <div class="state state-empty">
          <span>No dependencies found.</span>
        </div>
      {:else}
        <Tree
          nodes={nodes as TreeNode[]}
          {filter}
          selectable={(n: TreeNode) => !!n.selectable}
          rowClass={(ctx) => ctx.node.kind === 'section' ? 'tree-row-section' : ''}
          rowTitle={(n: TreeNode) => n.label}
        >
          {#snippet row({ node }: { node: TreeNode })}
            {#if node.icon}
              <span class="tree-icon"><PluginIcon name={node.icon} size={13} /></span>
            {/if}
            <span class="tree-label">{node.label}</span>
            {#if node.badge}
              <span class="tree-badge tree-badge-{node.badge_kind ?? 'muted'}">{node.badge}</span>
            {/if}
          {/snippet}
        </Tree>
      {/if}
    </div>
  </div>
</Modal>

<style>
  .header-title {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .title-text {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .title-sub {
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .modal-content {
    display: flex;
    flex-direction: column;
    height: min(640px, 70vh);
  }

  .card-toolbar {
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .search-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 3px 8px;
  }
  .search-wrap :global(.search-ic) { color: var(--text-muted); flex-shrink: 0; }
  .search-wrap input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    min-width: 0;
  }
  .clear {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 14px;
    cursor: pointer;
    padding: 0 4px;
  }
  .clear:hover { color: var(--text-primary); }

  .card-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    background: var(--bg-base);
  }

  .state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 40px 16px;
    color: var(--text-muted);
    font-size: 12px;
    text-align: center;
  }
  .state-error { color: var(--error); }
  .state :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
