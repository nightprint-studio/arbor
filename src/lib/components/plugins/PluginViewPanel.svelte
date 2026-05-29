<!--
  PluginViewPanel — renders the body of a plugin-registered main-area view.

  A view is a body surface (it occupies the area where the commit graph lives)
  rather than a side rail. The plugin registers it via `arbor.ui.add_view({id,
  …})` and responds to the `on_view_open` hook by pushing content through
  `arbor.ui.set_panel_content(id, {title, nodes, actions?})` — the SAME channel
  sidebar panels use, so a view shares the form-DSL content model.

  Unlike PluginSidebarPanel (which uses a lightweight node renderer), a view
  mounts the FULL `FormNodeRenderer`, so it gets parity with modals: every node
  type plus the dispatch / scoped-event / patch protocol (§1–4). High-frequency
  live updates flow through `arbor.ui.form.{patch,set_state_path,set_value,
  replace}` on the existing `plugin:form-update` channel — the mounted renderer
  applies them in place. `set_panel_content` is a full (re)build: we re-key the
  renderer when its serialized content changes so a rebuild reseeds cleanly.

  Opening is strictly non-blocking: we fire `on_view_open` and derive UI state
  from the store cache. `on_view_close` fires on unmount (toggle off, switch to
  another view, plugin reload).
-->
<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { X as XIcon } from 'lucide-svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { uiStore }           from '$lib/stores/ui.svelte';
  import { firePluginAction }  from '$lib/ipc/plugin';
  import { PANEL_CONTENT_POINT, findPanelContent } from '$lib/contributions/panel-content';
  import { setupTauriListeners } from '$lib/utils/tauri-listeners';
  import FormNodeRenderer from './FormNodeRenderer.svelte';
  import PluginIcon       from './PluginIcon.svelte';
  import Button           from '$lib/components/shared/ui/Button.svelte';
  import type { FormNode, ViewPlacement } from '$lib/types/plugin';

  interface Props {
    pluginName: string;
    viewId:     string;
    label?:     string;
    icon?:      string;
    placement?: ViewPlacement;
  }
  let { pluginName, viewId, label, icon, placement = 'graph' }: Props = $props();

  // Reactive view of the cached content. NEVER written from this component.
  const content = $derived(
    findPanelContent(contributionStore.forPoint(PANEL_CONTENT_POINT), pluginName, viewId)
  );
  const title   = $derived(content?.title ?? label ?? pluginName);
  const nodes   = $derived((content?.nodes ?? []) as FormNode[]);
  const actions = $derived((content?.actions ?? []) as any[]);

  // `set_panel_content` is a full (re)build — re-key the renderer when the
  // serialized content actually changes. `findPanelContent` allocates a fresh
  // object every call, so we compare a signature, not the reference. This is
  // low-frequency (hot updates use form.patch, which never touches the store).
  let rebuildEpoch = $state(0);
  let lastSig: string | null = null;
  $effect(() => {
    const sig = content
      ? JSON.stringify({ t: content.title, n: content.nodes, a: content.actions })
      : null;
    untrack(() => {
      // First observation seeds the baseline without a re-key — the initial
      // mount already reads the freshest nodes. Only a *subsequent* distinct
      // content push (a rebuild) bumps the epoch to remount the renderer.
      if (lastSig === null) { lastSig = sig; return; }
      if (sig !== null && sig !== lastSig) { lastSig = sig; rebuildEpoch++; }
    });
  });

  let renderer: ReturnType<typeof FormNodeRenderer> | null = $state(null);

  function fireViewHook(hook: 'on_view_open' | 'on_view_close') {
    firePluginAction(pluginName, hook, JSON.stringify({ view_id: viewId, label })).catch(() => {});
  }

  // Lifecycle is NON-reactive on purpose: a reactive $effect would re-fire
  // whenever a dependency churns, and `on_view_open` calls `set_panel_content`
  // → `contributions-changed` → AppShell's `activeView` re-derives → an
  // open/close loop. onMount/onDestroy fire exactly once per real mount; AppShell
  // keys this component on the view id, so switching views remounts (close→open).
  onMount(() => {
    fireViewHook('on_view_open');
    // A plugin reload wipes the runtime; re-fire so the view re-populates.
    const teardown = setupTauriListeners([
      { event: 'arbor://plugins-reloaded', handler: () => fireViewHook('on_view_open') },
    ]);
    return () => {
      teardown?.();
      fireViewHook('on_view_close');
    };
  });

  function close() { uiStore.setActiveMainView(null); }

  function footerVariant(a: any): 'primary' | 'secondary' | 'danger' {
    if (a?.variant === 'primary' || a?.variant === 'danger') return a.variant;
    return 'secondary';
  }
  function fireFooterAction(action: string | undefined) {
    if (!action) return;
    const values    = renderer?.getValues() ?? {};
    const liveState = renderer?.getLiveState();
    const payload: Record<string, unknown> = { ...values };
    if (liveState !== undefined) payload.state = liveState;
    firePluginAction(pluginName, action, JSON.stringify(payload)).catch(() => {});
  }
</script>

<div class="plugin-view" class:plugin-view-main={placement === 'main'}>
  <header class="pv-header">
    {#if icon}
      <PluginIcon name={icon} size={15} class="pv-header-icon" />
    {/if}
    <span class="pv-title">{title}</span>
    <button class="pv-close" type="button" onclick={close} aria-label="Close view">
      <XIcon size={15} />
    </button>
  </header>

  <div class="pv-body">
    {#if !content}
      <div class="pv-empty">Waiting for content…</div>
    {:else}
      {#key rebuildEpoch}
        <FormNodeRenderer
          bind:this={renderer}
          {pluginName}
          {nodes}
          onClose={close}
        />
      {/key}
    {/if}
  </div>

  {#if Array.isArray(actions) && actions.length > 0}
    <footer class="pv-footer">
      {#each actions as a, i (`${a.action ?? a.label ?? 'action'}:${i}`)}
        <Button variant={footerVariant(a)} onclick={() => fireFooterAction(a.action)}>
          {a.label ?? 'Action'}
        </Button>
      {/each}
    </footer>
  {/if}
</div>

<style>
  .plugin-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--bg-base);
  }

  .pv-header {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    flex-shrink: 0;
    padding: 0 8px 0 12px;
    border-bottom: 1px solid var(--border-subtle);
  }
  :global(.pv-header-icon) { color: var(--text-secondary); flex-shrink: 0; }
  .pv-title {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  .pv-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .pv-close:hover { background: var(--bg-hover); color: var(--text-primary); }

  .pv-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .pv-empty {
    padding: 24px;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    text-align: center;
  }

  .pv-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    flex-shrink: 0;
    padding: 8px 12px;
    border-top: 1px solid var(--border-subtle);
  }
</style>
